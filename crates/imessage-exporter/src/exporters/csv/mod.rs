//! Per-conversation CSV exporter.
//!
//! Column set follows the **HTML message surface** (guid, timestamp / read
//! receipt, sent/received + service, sender, subject, deleted, expressive,
//! shared location, bubble text, edited history, attachments, stickers,
//! tapbacks, app/placemark cards, replies, announcements). Values are filled
//! from `chat.db` so handles, full participant lists, and RFC 3339 times are
//! available even when HTML would omit them.
//!
//! Nested HTML structures (parts, tapbacks, edits, attachments, app cards,
//! participants) are encoded as JSON strings in cells.

mod data;

use std::{fs::File, io::BufWriter};

use serde::Serialize;

use crate::{
    app::{error::RuntimeError, runtime::Config},
    exporters::{
        formatter::{AttachmentRender, MessageFormatter, PartBodyBuilder, RenderContext},
        csv::data::{
            build_balloon_value, build_edit_records, build_part_records, expressive_label,
            optional_rfc3339, referenced_attachment_indices, shared_location_label,
            sticker_extras, transcription_for_attachment,
        },
        shared::{
            announcement::resolve_announcement,
            attachment::prepare_attachment,
            driver::{ExportState, MessageWriter},
            part::AttachmentResolver,
            time::message_time,
        },
    },
};

use imessage_database::{
    message_types::{
        edited::EditedMessage,
        variants::{Announcement, Tapback, TapbackAction, Variant},
    },
    tables::{
        attachment::Attachment,
        chat::Chat,
        messages::{
            Message,
            models::{AttachmentMeta, AttributedRange, GroupAction, Service, SharedLocation},
        },
        table::YOU,
    },
};

/// CSV column header order (HTML-driven field inventory).
const HEADERS: &[&str] = &[
    "chat_identifier",
    "conversation_type",
    "group_title",
    "participants_json",
    "guid",
    "timestamp",
    "timestamp_utc",
    "timestamp_display",
    "read_receipt",
    "direction",
    "service",
    "sender_handle",
    "sender_display_name",
    "subject",
    "text",
    "is_deleted",
    "send_effect",
    "shared_location",
    "is_announcement",
    "announcement",
    "is_reply",
    "thread_originator_guid",
    "thread_originator_part",
    "num_replies",
    "parts_json",
    "edits_json",
    "attachments_json",
    "tapbacks_json",
    "app_json",
    "export_source",
    "export_tool",
    "export_tool_version",
];

const EXPORT_SOURCE: &str = "imessage";
const EXPORT_TOOL: &str = "iMessage Exporter";
const EXPORT_TOOL_VERSION: &str = "4.2.0";

fn parse_thread_part(part: &str) -> Option<usize> {
    part.split(':').next().and_then(|p| p.parse::<usize>().ok())
}

fn json_cell(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

// MARK: Row
#[derive(Debug, Serialize)]
struct CsvRow {
    chat_identifier: String,
    conversation_type: String,
    group_title: String,
    participants_json: String,
    guid: String,
    timestamp: String,
    timestamp_utc: String,
    timestamp_display: String,
    read_receipt: String,
    direction: String,
    service: String,
    sender_handle: String,
    sender_display_name: String,
    subject: String,
    text: String,
    is_deleted: bool,
    send_effect: String,
    shared_location: String,
    is_announcement: bool,
    announcement: String,
    is_reply: bool,
    thread_originator_guid: String,
    thread_originator_part: String,
    num_replies: i64,
    parts_json: String,
    edits_json: String,
    attachments_json: String,
    tapbacks_json: String,
    app_json: String,
    export_source: String,
    export_tool: String,
    export_tool_version: String,
}

#[derive(Debug, Serialize)]
struct ParticipantCell {
    handle: String,
    display_name: String,
}

#[derive(Debug, Serialize)]
struct AttachmentCell {
    path: Option<String>,
    original_name: Option<String>,
    mime_type: Option<String>,
    is_sticker: bool,
    transcription: Option<String>,
    sticker_effect: Option<String>,
}

#[derive(Debug, Serialize)]
struct TapbackCell {
    part_index: usize,
    kind: &'static str,
    emoji: Option<String>,
    reactor_handle: Option<String>,
    reactor_display_name: Option<String>,
}

// MARK: CSV
pub struct CSV<'a> {
    pub config: &'a Config,
    pub state: ExportState,
}

impl<'a> CSV<'a> {
    pub fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        Ok(CSV {
            config,
            state: ExportState::new(config, "csv")?,
        })
    }

    fn raw_handle(&self, handle_id: i32) -> Option<String> {
        self.config
            .real_participants
            .get(&handle_id)
            .and_then(|internal| self.config.participants.get(internal))
            .map(|name| name.details.clone())
    }

    fn display_name_for(&self, handle_id: i32) -> Option<String> {
        self.config
            .real_participants
            .get(&handle_id)
            .and_then(|internal| self.config.participants.get(internal))
            .map(|name| {
                if name.full.is_empty() {
                    name.details.clone()
                } else {
                    name.full.clone()
                }
            })
    }

    fn participants_for(&self, chatroom: &Chat) -> (Vec<ParticipantCell>, &'static str) {
        let mut records = Vec::new();
        let mut count = 0;
        if let Some(handles) = self.config.chatroom_participants.get(&chatroom.rowid) {
            count = handles.len();
            for handle_id in handles {
                let name = self
                    .config
                    .real_participants
                    .get(handle_id)
                    .and_then(|internal| self.config.participants.get(internal));
                let (handle, display_name) = match name {
                    Some(n) => (
                        n.details.clone(),
                        if n.full.is_empty() {
                            String::new()
                        } else {
                            n.full.clone()
                        },
                    ),
                    None => (String::new(), String::new()),
                };
                records.push(ParticipantCell {
                    handle,
                    display_name,
                });
            }
        }
        let conversation_type = if count > 1 { "group" } else { "individual" };
        (records, conversation_type)
    }

    fn chat_meta(&self, message: &Message) -> (String, String, String, String) {
        let Some((chatroom, _)) = self.config.conversation(message) else {
            return (
                String::new(),
                String::new(),
                String::new(),
                "[]".to_string(),
            );
        };
        let (participants, conversation_type) = self.participants_for(chatroom);
        (
            chatroom.chat_identifier.clone(),
            conversation_type.to_string(),
            chatroom.display_name().unwrap_or_default().to_string(),
            json_cell(&participants),
        )
    }

    fn announcement_text(&self, msg: &Message) -> Option<String> {
        let resolved = resolve_announcement(msg, self.config, YOU)?;
        let body = match &resolved.announcement {
            Announcement::AudioMessageKept => "kept an audio message.".to_string(),
            Announcement::FullyUnsent => "unsent a message!".to_string(),
            Announcement::Unknown(num) => format!("performed unknown action {num}."),
            Announcement::GroupAction(group) => match group {
                GroupAction::ParticipantAdded(_) => {
                    format!("added {} to the conversation.", resolved.participant_name)
                }
                GroupAction::ParticipantRemoved(_) => {
                    format!(
                        "removed {} from the conversation.",
                        resolved.participant_name
                    )
                }
                GroupAction::NameChange(name) => format!("named the conversation {name}"),
                GroupAction::ParticipantLeft => "left the conversation.".to_string(),
                GroupAction::GroupIconChanged => "changed the group photo.".to_string(),
                GroupAction::GroupIconRemoved => "removed the group photo.".to_string(),
                GroupAction::ChatBackgroundChanged => "changed the chat background.".to_string(),
                GroupAction::ChatBackgroundRemoved => "removed the chat background.".to_string(),
                GroupAction::PhoneNumberChanged(_) => "changed their phone number.".to_string(),
            },
        };
        Some(format!("{} {body}", resolved.who))
    }

    fn build_tapback_cells(&self, message: &Message) -> Vec<TapbackCell> {
        let Some(parts) = self.config.tapbacks.get(&message.guid) else {
            return Vec::new();
        };

        let mut sortable: Vec<(usize, i64, i32, TapbackCell)> = Vec::new();
        for (&part_index, tapbacks) in parts {
            for tapback in tapbacks {
                let Variant::Tapback(_, action, kind) = tapback.variant() else {
                    continue;
                };
                if matches!(action, TapbackAction::Removed) {
                    continue;
                }
                let (kind, emoji) = match kind {
                    Tapback::Loved => ("loved", None),
                    Tapback::Liked => ("liked", None),
                    Tapback::Disliked => ("disliked", None),
                    Tapback::Laughed => ("laughed", None),
                    Tapback::Emphasized => ("emphasized", None),
                    Tapback::Questioned => ("questioned", None),
                    Tapback::Emoji(e) => ("emoji", e.map(str::to_string)),
                    Tapback::Sticker => ("sticker", None),
                };
                let (reactor_handle, reactor_display_name) = if tapback.is_from_me() {
                    (None, Some(self.config.options.custom_name.clone().unwrap_or_else(|| "Me".into())))
                } else if let Some(handle_id) = tapback.handle_id {
                    (
                        self.raw_handle(handle_id),
                        self.display_name_for(handle_id),
                    )
                } else {
                    (None, None)
                };
                sortable.push((
                    part_index,
                    tapback.date,
                    tapback.rowid,
                    TapbackCell {
                        part_index,
                        kind,
                        emoji,
                        reactor_handle,
                        reactor_display_name,
                    },
                ));
            }
        }
        sortable.sort_by_key(|(part, date, rowid, _)| (*part, *date, *rowid));
        sortable.into_iter().map(|(_, _, _, cell)| cell).collect()
    }

    fn build_row(&self, message: &Message) -> Result<CsvRow, RuntimeError> {
        let (chat_identifier, conversation_type, group_title, participants_json) =
            self.chat_meta(message);

        let (timestamp, timestamp_utc) = optional_rfc3339(message.date(self.config.offset));
        let (timestamp_display, read_receipt) = message_time(self.config, message);

        let is_from_me = message.is_from_me();
        let direction = if is_from_me { "outgoing" } else { "incoming" };

        let (sender_handle, sender_display_name) = if is_from_me {
            (
                String::new(),
                self.config
                    .options
                    .custom_name
                    .clone()
                    .unwrap_or_else(|| "Me".into()),
            )
        } else if let Some(handle_id) = message.handle_id {
            (
                self.raw_handle(handle_id).unwrap_or_default(),
                self.display_name_for(handle_id).unwrap_or_default(),
            )
        } else {
            (String::new(), String::new())
        };

        let service = match message.service() {
            Service::Unknown => String::new(),
            other => other.to_string(),
        };

        let is_announcement = message.is_announcement();
        let announcement = if is_announcement {
            self.announcement_text(message).unwrap_or_default()
        } else {
            String::new()
        };

        let is_reply = message.is_reply();
        let thread_originator_guid = if is_reply {
            message.thread_originator_guid.clone().unwrap_or_default()
        } else {
            String::new()
        };
        let thread_originator_part = if is_reply {
            message
                .thread_originator_part
                .as_deref()
                .and_then(parse_thread_part)
                .unwrap_or(0)
                .to_string()
        } else {
            String::new()
        };

        let send_effect = expressive_label(message.get_expressive()).unwrap_or_default();
        let shared_location = message
            .shared_location_kind()
            .map(shared_location_label)
            .unwrap_or_default()
            .to_string();

        let edits = message
            .edited_parts
            .as_ref()
            .map(|edited| build_edit_records(edited, &self.config.offset))
            .unwrap_or_default();

        let app = build_balloon_value(self.config, message);

        let mut attachment_cells = Vec::new();
        let mut attachments = Attachment::from_message(self.config.data_source.db(), message)?;
        let referenced = referenced_attachment_indices(message, &attachments);
        let emitted_index: std::collections::HashMap<usize, usize> = referenced
            .iter()
            .enumerate()
            .map(|(emitted, &full)| (full, emitted))
            .collect();

        let mut parts = build_part_records(message, &attachments);
        for part in &mut parts {
            part.attachment_indices = part
                .attachment_indices
                .iter()
                .filter_map(|full| emitted_index.get(full).copied())
                .collect();
        }

        for &idx in &referenced {
            let attachment = &mut attachments[idx];
            let is_sticker = attachment.is_sticker;
            let transcription = transcription_for_attachment(message, attachment);
            let (_genmoji, sticker_effect) = sticker_extras(self.config, attachment);
            let _ = prepare_attachment(self.config, &self.state, attachment, message);
            attachment_cells.push(AttachmentCell {
                path: Some(self.config.message_attachment_path(attachment)),
                original_name: attachment.transfer_name.clone(),
                mime_type: attachment.mime_type.clone(),
                is_sticker,
                transcription,
                sticker_effect,
            });
        }

        let tapbacks = self.build_tapback_cells(message);

        Ok(CsvRow {
            chat_identifier,
            conversation_type,
            group_title,
            participants_json,
            guid: message.guid.clone(),
            timestamp: timestamp.unwrap_or_default(),
            timestamp_utc: timestamp_utc.unwrap_or_default(),
            timestamp_display,
            read_receipt,
            direction: direction.to_string(),
            service,
            sender_handle,
            sender_display_name,
            subject: message.subject.clone().unwrap_or_default(),
            text: message.text.clone().unwrap_or_default(),
            is_deleted: message.is_deleted(),
            send_effect,
            shared_location,
            is_announcement,
            announcement,
            is_reply,
            thread_originator_guid,
            thread_originator_part,
            num_replies: i64::from(message.num_replies),
            parts_json: json_cell(&parts),
            edits_json: json_cell(&edits),
            attachments_json: json_cell(&attachment_cells),
            tapbacks_json: json_cell(&tapbacks),
            app_json: app.map(|v| json_cell(&v)).unwrap_or_else(|| "null".into()),
            export_source: EXPORT_SOURCE.to_string(),
            export_tool: EXPORT_TOOL.to_string(),
            export_tool_version: EXPORT_TOOL_VERSION.to_string(),
        })
    }

    fn write_row(&self, message: &Message, out: &mut String) -> Result<(), RuntimeError> {
        let row = self.build_row(message)?;
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(Vec::new());
        wtr.serialize(&row).map_err(std::io::Error::other)?;
        let bytes = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        out.push_str(std::str::from_utf8(&bytes).map_err(std::io::Error::other)?);
        Ok(())
    }
}

// MARK: Driver hooks
impl<'a> MessageWriter<'a> for CSV<'a> {
    const LABEL: &'static str = "csv";
    const BUFFER_CAPACITY: usize = 1024;

    fn config(&self) -> &'a Config {
        self.config
    }

    fn state(&self) -> &ExportState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ExportState {
        &mut self.state
    }

    fn write_file_header(file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(Vec::new());
        wtr.write_record(HEADERS).map_err(std::io::Error::other)?;
        let bytes = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        use std::io::Write;
        file.write_all(&bytes)?;
        Ok(())
    }

    fn write_file_footer(_file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn footer_notice() -> Option<&'static str> {
        None
    }
}

// MARK: Writer
impl<'a> MessageFormatter<'a> for CSV<'a> {
    fn format_message_into(
        &self,
        message: &Message,
        _context: RenderContext,
        out: &mut String,
    ) -> Result<(), RuntimeError> {
        self.write_row(message, out)
    }

    fn format_announcement(&self, msg: &Message, out: &mut String) {
        let _ = self.write_row(msg, out);
    }

    fn format_attachment(
        &self,
        _attachment: &'a mut Attachment,
        _msg: &'a Message,
        _metadata: &AttachmentMeta,
    ) -> AttachmentRender {
        AttachmentRender::MissingFilename
    }

    fn format_sticker(&self, _attachment: &'a mut Attachment, _msg: &'a Message) -> String {
        String::new()
    }

    fn format_app(
        &self,
        _msg: &'a Message,
        _attachments: &mut Vec<Attachment>,
    ) -> Result<String, RuntimeError> {
        Ok(String::new())
    }

    fn format_tapback(&self, _msg: &Message) -> Result<String, RuntimeError> {
        Ok(String::new())
    }

    fn format_shareplay(&self) -> &'static str {
        ""
    }

    fn format_shared_location(&self, _kind: SharedLocation) -> &'static str {
        ""
    }

    fn format_edited(
        &'a self,
        _msg: &'a Message,
        _edited_message: &'a EditedMessage,
        _message_part_idx: usize,
        _attachments: &'a mut Vec<Attachment>,
        _resolver: &mut AttachmentResolver,
    ) -> Option<String> {
        None
    }

    fn format_attributes(&self, text: &str, _ranges: &[AttributedRange]) -> String {
        text.to_string()
    }

    fn render_run(
        &'a self,
        _message: &'a Message,
        _ranges: &'a [AttributedRange],
        _attachments: &'a mut Vec<Attachment>,
        _resolver: &mut AttachmentResolver,
    ) -> <Self as PartBodyBuilder>::Body
    where
        Self: PartBodyBuilder,
    {
        unreachable!("CSV exporter does not implement PartBodyBuilder")
    }
}

#[cfg(test)]
mod tests {
    use super::{HEADERS, CsvRow};

    #[test]
    fn csv_header_matches_row_fields() {
        assert_eq!(HEADERS.len(), 32);
        assert_eq!(HEADERS[0], "chat_identifier");
        assert_eq!(HEADERS[9], "direction");
        assert_eq!(HEADERS[HEADERS.len() - 1], "export_tool_version");
    }

    #[test]
    fn csv_row_serializes_html_surface_fields() {
        let row = CsvRow {
            chat_identifier: "+15551212".into(),
            conversation_type: "individual".into(),
            group_title: String::new(),
            participants_json: r#"[{"handle":"+15551212","display_name":"Ada"}]"#.into(),
            guid: "TEST-GUID".into(),
            timestamp: "2019-11-21T12:50:31-05:00".into(),
            timestamp_utc: "2019-11-21T17:50:31Z".into(),
            timestamp_display: "Nov 21, 2019 12:50:31 PM".into(),
            read_receipt: "(Read by you after 12 seconds)".into(),
            direction: "incoming".into(),
            service: "iMessage".into(),
            sender_handle: "+15551212".into(),
            sender_display_name: "Ada".into(),
            subject: String::new(),
            text: "hello".into(),
            is_deleted: false,
            send_effect: String::new(),
            shared_location: String::new(),
            is_announcement: false,
            announcement: String::new(),
            is_reply: false,
            thread_originator_guid: String::new(),
            thread_originator_part: String::new(),
            num_replies: 0,
            parts_json: "[]".into(),
            edits_json: "[]".into(),
            attachments_json: "[]".into(),
            tapbacks_json: "[]".into(),
            app_json: "null".into(),
            export_source: "imessage".into(),
            export_tool: "iMessage Exporter".into(),
            export_tool_version: "4.2.0".into(),
        };

        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(Vec::new());
        wtr.serialize(&row).unwrap();
        let s = String::from_utf8(wtr.into_inner().unwrap()).unwrap();
        assert!(s.contains("incoming"));
        assert!(s.contains("TEST-GUID"));
        assert!(s.contains("hello"));
        assert!(s.contains("+15551212"));
        assert!(s.contains("Nov 21, 2019 12:50:31 PM"));
        assert!(s.contains("imessage"));
    }

    #[test]
    fn csv_serialize_headers_via_writer() {
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(Vec::new());
        wtr.write_record(HEADERS).unwrap();
        let bytes = wtr.into_inner().unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.starts_with("chat_identifier,"));
        assert!(s.contains("direction"));
        assert!(s.contains("tapbacks_json"));
        assert!(s.contains("app_json"));
    }
}
