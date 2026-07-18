//! Newline-delimited JSON (NDJSON) exporter.
//!
//! Each conversation is written to its own `.json` file as a stream of
//! newline-delimited JSON objects:
//!
//! - The **first line** is a `"conversation"` record carrying conversation-level
//!   metadata (chat identifier, service, type, group title, and the raw
//!   participant handles).
//! - Every **subsequent line** is a `"message"` record.
//!
//! Unlike the HTML/TXT exporters, this format intentionally emits *raw handles*
//! (phone numbers / emails) and an explicit `is_from_me` flag rather than
//! display names, so downstream consumers can resolve names from their own
//! contact source instead of reverse-engineering them from rendered output.
//!
//! Message data that downstream archives consume (plain text, attachments,
//! announcements, tapbacks, reply threading, edit history, multipart body
//! parts, and best-effort balloon/app payloads) is represented. Tapbacks are
//! attached inline to the message they react to, and reply threading is
//! emitted as flat metadata (`thread_originator_guid` / `thread_originator_part`
//! plus `num_replies`) so consumers can reconstruct threads themselves — every
//! reply is also written as its own top-level `message` record in chronological
//! order. Presentation-only HTML/TXT concerns (CSS, nested reply UI, markup
//! tags) are not emitted; the corresponding [`MessageFormatter`] hooks remain
//! minimal stubs.

use std::{fs::File, io::BufWriter};

use chrono::Local;
use serde::Serialize;
use serde_json::Value;

use crate::{
    app::{error::RuntimeError, runtime::Config},
    exporters::{
        formatter::{AttachmentRender, MessageFormatter, PartBodyBuilder, RenderContext},
        json::data::{
            EditEventRecord, PartRecord, build_balloon_value, build_edit_records,
            build_part_records, expressive_label, optional_rfc3339, shared_location_label,
            sticker_extras, transcription_for_attachment,
        },
        shared::{
            announcement::resolve_announcement,
            attachment::prepare_attachment,
            driver::{ExportState, MessageWriter},
            part::{AttachmentResolver, referenced_attachment_indices},
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

mod data;

/// Version of the emitted JSON schema. Bump on additive or breaking changes to
/// the record shapes so consumers can detect incompatibilities.
const SCHEMA_VERSION: u32 = 4;

/// Parse the body component index from a `thread_originator_part` field.
///
/// The field is stored as a colon-separated string (e.g. `"0:0:0"`); the
/// first segment is the targeted body component index.
fn parse_thread_part(part: &str) -> Option<usize> {
    part.split(':').next().and_then(|p| p.parse::<usize>().ok())
}

// MARK: Records
/// First line of each conversation file: conversation-level metadata.
#[derive(Serialize)]
struct ConversationRecord<'a> {
    record: &'static str,
    schema_version: u32,
    /// Stable identifier for the chat (phone, email, or group chat GUID).
    chat_identifier: &'a str,
    /// Service the chat used, e.g. `iMessage` or `SMS`.
    service: Option<&'a str>,
    /// `"individual"` or `"group"`.
    conversation_type: &'static str,
    /// Custom group name, if one is set.
    group_title: Option<&'a str>,
    /// Raw participant handles plus an advisory resolved name hint.
    participants: Vec<ParticipantRecord>,
    /// When the export was produced (RFC 3339, local time).
    exported_at: &'a str,
}

/// A single conversation participant.
#[derive(Serialize)]
struct ParticipantRecord {
    /// Raw handle (phone number or email) as stored by iMessage.
    handle: String,
    /// Exporter-resolved full name, if any. Advisory only.
    name_hint: String,
}

/// One message line in a conversation file.
#[derive(Serialize)]
struct MessageRecord {
    record: &'static str,
    guid: String,
    /// RFC 3339 timestamp in the database's local time zone.
    timestamp: Option<String>,
    /// RFC 3339 timestamp normalized to UTC.
    timestamp_utc: Option<String>,
    /// When the message was read (RFC 3339 local), if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp_read: Option<String>,
    /// When the message was delivered (RFC 3339 local), if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp_delivered: Option<String>,
    is_from_me: bool,
    /// Raw sender handle. `null` when `is_from_me` is `true`.
    sender: Option<String>,
    /// Per-message service (`iMessage`, `SMS`, `RCS`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<String>,
    subject: Option<String>,
    text: Option<String>,
    is_announcement: bool,
    /// Human-readable announcement description, if this is an announcement.
    announcement: Option<String>,
    /// `true` when the message was deleted from the conversation.
    is_deleted: bool,
    /// Bubble/screen effect label, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    expressive: Option<String>,
    /// Legacy shared-location start/stop marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    shared_location: Option<&'static str>,
    /// Best-effort structured balloon/app payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    balloon: Option<Value>,
    /// Multipart body components after `apply_body`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parts: Vec<PartRecord>,
    /// Edit / unsend history keyed by body part index.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    edits: Vec<EditEventRecord>,
    attachments: Vec<AttachmentRecord>,
    /// Tapbacks (reactions) applied to this message. Only currently-active
    /// tapbacks are listed; ones that were later removed are omitted.
    tapbacks: Vec<TapbackRecord>,
    /// `true` when this message is a reply within a thread.
    is_reply: bool,
    /// GUID of the message this one replies to. `null` unless `is_reply`.
    thread_originator_guid: Option<String>,
    /// Body component index of the originator message targeted by this reply.
    /// `null` unless `is_reply`.
    thread_originator_part: Option<usize>,
    /// Number of replies threaded under this message.
    num_replies: i32,
}

/// A single tapback (reaction) applied to a message.
#[derive(Serialize)]
struct TapbackRecord {
    /// Body component index of the target message this tapback applies to.
    part_index: usize,
    /// Reaction kind: `loved`, `liked`, `disliked`, `laughed`, `emphasized`,
    /// `questioned`, `emoji`, or `sticker`.
    kind: &'static str,
    /// Emoji used, for custom-emoji tapbacks only.
    emoji: Option<String>,
    is_from_me: bool,
    /// Raw handle of the reactor. `null` when `is_from_me` is `true`.
    sender: Option<String>,
}

/// A single attachment reference within a message.
#[derive(Serialize)]
struct AttachmentRecord {
    /// Path to the exported file, relative to the export directory when copied.
    path: String,
    /// Original filename as sent/received.
    original_name: Option<String>,
    mime_type: Option<String>,
    is_sticker: bool,
    /// Audio transcription, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    transcription: Option<String>,
    /// Genmoji / emoji-image short description, when present on stickers.
    #[serde(skip_serializing_if = "Option::is_none")]
    genmoji_prompt: Option<String>,
    /// Parsed sticker effect name, when readable from the attachment bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    sticker_effect: Option<String>,
}

// MARK: JSON
pub struct JSON<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Shared per-export state (file cache, orphaned writer, progress bar).
    pub state: ExportState,
    /// RFC 3339 timestamp captured once when the export begins.
    exported_at: String,
}

impl<'a> JSON<'a> {
    pub fn new(config: &'a Config) -> Result<Self, RuntimeError> {
        Ok(JSON {
            config,
            state: ExportState::new(config, "json")?,
            exported_at: Local::now().to_rfc3339(),
        })
    }

    /// Resolve a handle ID to its raw handle string (phone/email).
    fn raw_handle(&self, handle_id: i32) -> Option<String> {
        self.config
            .real_participants
            .get(&handle_id)
            .and_then(|internal| self.config.participants.get(internal))
            .map(|name| name.details.clone())
    }

    /// Build the conversation's participant list and infer its type.
    fn participants_for(&self, chatroom: &Chat) -> (Vec<ParticipantRecord>, &'static str) {
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
                let (handle, name_hint) = match name {
                    Some(n) => (n.details.clone(), n.full.clone()),
                    None => (String::new(), String::new()),
                };
                records.push(ParticipantRecord { handle, name_hint });
            }
        }
        // A 1:1 chat has a single other participant; anything more is a group.
        let conversation_type = if count > 1 { "group" } else { "individual" };
        (records, conversation_type)
    }

    /// Render an announcement message into a human-readable description.
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

    /// Collect the active tapbacks applied to `message`, sorted deterministically.
    ///
    /// Tapbacks are read from the pre-built [`Config::tapbacks`] cache, keyed by
    /// target GUID and body component index. Removed tapbacks are skipped, since
    /// the database only retains the latest tapback state.
    fn build_tapback_records(&self, message: &Message) -> Vec<TapbackRecord> {
        let Some(parts) = self.config.tapbacks.get(&message.guid) else {
            return Vec::new();
        };

        // Collect with sort keys so the output order is stable regardless of
        // `HashMap` iteration order: by target part, then chronologically.
        let mut sortable: Vec<(usize, i64, i32, TapbackRecord)> = Vec::new();
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
                let is_from_me = tapback.is_from_me();
                let sender = if is_from_me {
                    None
                } else {
                    tapback.handle_id.and_then(|handle| self.raw_handle(handle))
                };
                sortable.push((
                    part_index,
                    tapback.date,
                    tapback.rowid,
                    TapbackRecord {
                        part_index,
                        kind,
                        emoji,
                        is_from_me,
                        sender,
                    },
                ));
            }
        }
        sortable.sort_by_key(|(part, date, rowid, _)| (*part, *date, *rowid));
        sortable.into_iter().map(|(_, _, _, record)| record).collect()
    }

    /// Assemble a [`MessageRecord`], copying any attachments along the way.
    fn build_message_record(&self, message: &Message) -> Result<MessageRecord, RuntimeError> {
        let (timestamp, timestamp_utc) = optional_rfc3339(message.date(self.config.offset));

        let timestamp_read = if message.date_read != 0 {
            optional_rfc3339(message.date_read(self.config.offset)).0
        } else {
            None
        };
        let timestamp_delivered = if message.date_delivered != 0 {
            optional_rfc3339(message.date_delivered(self.config.offset)).0
        } else {
            None
        };

        let is_from_me = message.is_from_me();
        let sender = if is_from_me {
            None
        } else {
            message.handle_id.and_then(|handle| self.raw_handle(handle))
        };

        let service = match message.service() {
            Service::Unknown => None,
            other => Some(other.to_string()),
        };

        let is_announcement = message.is_announcement();
        let announcement = if is_announcement {
            self.announcement_text(message)
        } else {
            None
        };

        let is_reply = message.is_reply();
        let thread_originator_guid = if is_reply {
            message.thread_originator_guid.clone()
        } else {
            None
        };
        let thread_originator_part = if is_reply {
            Some(
                message
                    .thread_originator_part
                    .as_deref()
                    .and_then(parse_thread_part)
                    .unwrap_or(0),
            )
        } else {
            None
        };

        let tapbacks = self.build_tapback_records(message);
        let expressive = expressive_label(message.get_expressive());
        let shared_location = message
            .shared_location_kind()
            .map(shared_location_label);
        let balloon = build_balloon_value(self.config, message);
        let edits = message
            .edited_parts
            .as_ref()
            .map(|edited| build_edit_records(edited, &self.config.offset))
            .unwrap_or_default();

        let mut attachment_records = Vec::new();
        let mut attachments = Attachment::from_message(self.config.data_source.db(), message)?;
        let referenced = referenced_attachment_indices(message, &attachments);
        // Map full attachment-list indices → positions in the emitted `attachments` array.
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
            let (genmoji_prompt, sticker_effect) = sticker_extras(self.config, attachment);
            // Copy/convert the file; failures still record the known metadata.
            let _ = prepare_attachment(
                self.config,
                &self.state,
                attachment,
                message,
                true,
            );
            attachment_records.push(AttachmentRecord {
                path: self.config.message_attachment_path(attachment),
                original_name: attachment.transfer_name.clone(),
                mime_type: attachment.mime_type.clone(),
                is_sticker,
                transcription,
                genmoji_prompt,
                sticker_effect,
            });
        }

        Ok(MessageRecord {
            record: "message",
            guid: message.guid.clone(),
            timestamp,
            timestamp_utc,
            timestamp_read,
            timestamp_delivered,
            is_from_me,
            sender,
            service,
            subject: message.subject.clone(),
            text: message.text.clone(),
            is_announcement,
            announcement,
            is_deleted: message.is_deleted(),
            expressive,
            shared_location,
            balloon,
            parts,
            edits,
            attachments: attachment_records,
            tapbacks,
            is_reply,
            thread_originator_guid,
            thread_originator_part,
            num_replies: message.num_replies,
        })
    }

    /// Serialize a [`MessageRecord`] as one NDJSON line into `out`.
    fn write_message_record(&self, message: &Message, out: &mut String) -> Result<(), RuntimeError> {
        let record = self.build_message_record(message)?;
        // TODO: Some exports contain a raw (unescaped) newline byte inside a JSON
        // string value — e.g. a pasted multi-line address in `text` — which splits
        // the NDJSON record across physical lines and breaks line-based parsers.
        // `serde_json::to_string` should escape control characters; investigate whether
        // `message.text` is mutated after serialization or written through another path.
        let line = serde_json::to_string(&record).map_err(std::io::Error::other)?;
        out.push_str(&line);
        out.push('\n');
        Ok(())
    }
}

// MARK: Driver hooks
impl<'a> MessageWriter<'a> for JSON<'a> {
    const LABEL: &'static str = "json";
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

    fn write_file_header(_file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn write_file_footer(_file: &mut BufWriter<File>) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn footer_notice() -> Option<&'static str> {
        None
    }

    fn conversation_header(&self, chatroom: &Chat) -> Option<String> {
        let (participants, conversation_type) = self.participants_for(chatroom);
        let record = ConversationRecord {
            record: "conversation",
            schema_version: SCHEMA_VERSION,
            chat_identifier: &chatroom.chat_identifier,
            service: chatroom.service_name.as_deref(),
            conversation_type,
            group_title: chatroom.display_name(),
            participants,
            exported_at: &self.exported_at,
        };
        let mut line = serde_json::to_string(&record).ok()?;
        line.push('\n');
        Some(line)
    }
}

// MARK: Writer
impl<'a> MessageFormatter<'a> for JSON<'a> {
    fn format_message_into(
        &self,
        message: &Message,
        _context: RenderContext,
        out: &mut String,
    ) -> Result<(), RuntimeError> {
        self.write_message_record(message, out)
    }

    fn format_announcement(&self, msg: &Message, out: &mut String) {
        // Best-effort: serialization of our own owned types does not fail in
        // practice, so a failure here simply emits nothing for this message.
        let _ = self.write_message_record(msg, out);
    }

    // The remaining hooks exist only for the HTML/TXT presentation paths and
    // are never invoked by the JSON exporter, which builds records directly.

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

    // `render_run` is only reachable for exporters that implement
    // [`PartBodyBuilder`]. The JSON exporter builds records directly and does
    // not, so this is never called; it exists solely to satisfy the trait.
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
        unreachable!("JSON exporter does not implement PartBodyBuilder")
    }
}

// MARK: Tests
#[cfg(test)]
mod tests {
    use serde_json::Value;

    use imessage_database::tables::{chat::Chat, messages::Message};

    use crate::{
        Config, JSON, Options,
        app::{contacts::Name, export_type::ExportType},
        exporters::{
            formatter::{MessageFormatter, RenderContext},
            shared::driver::MessageWriter,
        },
    };

    fn fake_config() -> Config {
        Config::fake_app(Options::fake_options(ExportType::Json))
    }

    #[test]
    fn message_from_me_emits_null_sender_and_text() {
        let config = fake_config();
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = "ABC-123".to_string();
        message.text = Some("Hello world".to_string());
        message.is_from_me = true;

        let mut out = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut out)
            .unwrap();

        assert!(out.ends_with('\n'));
        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(value["record"], "message");
        assert_eq!(value["guid"], "ABC-123");
        assert_eq!(value["text"], "Hello world");
        assert_eq!(value["is_from_me"], true);
        assert!(value["sender"].is_null());
        assert_eq!(value["is_announcement"], false);
        assert!(value["timestamp"].is_string());
        assert!(value["timestamp_utc"].is_string());
        assert!(value["attachments"].as_array().unwrap().is_empty());
        assert!(value["tapbacks"].as_array().unwrap().is_empty());
        assert_eq!(value["is_reply"], false);
        assert!(value["thread_originator_guid"].is_null());
        assert!(value["thread_originator_part"].is_null());
        assert_eq!(value["num_replies"], 0);
    }

    #[test]
    fn message_from_them_emits_raw_handle_sender() {
        let mut config = fake_config();
        config.participants.insert(10, Name::fake_name("+15551234567"));
        config.real_participants.insert(10, 10);
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.text = Some("Hi".to_string());
        message.is_from_me = false;
        message.handle_id = Some(10);

        let mut out = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut out)
            .unwrap();

        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(value["is_from_me"], false);
        assert_eq!(value["sender"], "+15551234567");
    }

    #[test]
    fn announcement_emits_message_record_with_text() {
        let config = fake_config();
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.is_from_me = true;
        // item_type 2 with a group title is a name-change announcement
        message.item_type = 2;
        message.group_title = Some("Trip 2026".to_string());

        let mut out = String::new();
        exporter.format_announcement(&message, &mut out);

        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(value["record"], "message");
        assert_eq!(value["is_announcement"], true);
        assert_eq!(
            value["announcement"],
            "You named the conversation Trip 2026"
        );
    }

    /// Build a tapback message targeting `target_guid` part `part`.
    fn fake_tapback(guid: &str, target_guid: &str, part: usize, assoc_type: i32) -> Message {
        let mut tapback = Config::fake_message();
        tapback.guid = guid.to_string();
        tapback.is_from_me = false;
        tapback.handle_id = Some(10);
        tapback.associated_message_type = Some(assoc_type);
        tapback.associated_message_guid = Some(format!("p:{part}/{target_guid}"));
        tapback
    }

    #[test]
    fn message_with_tapbacks_lists_active_reactions() {
        let mut config = fake_config();
        config.participants.insert(10, Name::fake_name("+15551234567"));
        config.real_participants.insert(10, 10);

        // A "Loved" (2000) tapback plus a since-removed "Liked" (3001) tapback.
        let mut by_part = std::collections::HashMap::new();
        by_part.insert(
            0usize,
            vec![
                fake_tapback("TB-LOVE", "ABC-123", 0, 2000),
                fake_tapback("TB-LIKE", "ABC-123", 0, 3001),
            ],
        );
        config.tapbacks.insert("ABC-123".to_string(), by_part);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = "ABC-123".to_string();
        message.text = Some("Hello".to_string());
        message.is_from_me = true;

        let mut out = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut out)
            .unwrap();

        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        let tapbacks = value["tapbacks"].as_array().unwrap();
        // The removed "Liked" tapback is omitted; only the active "Loved" remains.
        assert_eq!(tapbacks.len(), 1);
        assert_eq!(tapbacks[0]["kind"], "loved");
        assert_eq!(tapbacks[0]["part_index"], 0);
        assert_eq!(tapbacks[0]["is_from_me"], false);
        assert_eq!(tapbacks[0]["sender"], "+15551234567");
        assert!(tapbacks[0]["emoji"].is_null());
    }

    #[test]
    fn custom_emoji_tapback_includes_emoji() {
        let mut config = fake_config();
        let mut tapback = fake_tapback("TB-EMOJI", "ABC-123", 1, 2006);
        tapback.associated_message_emoji = Some("🎉".to_string());
        tapback.is_from_me = true;
        tapback.handle_id = None;

        let mut by_part = std::collections::HashMap::new();
        by_part.insert(1usize, vec![tapback]);
        config.tapbacks.insert("ABC-123".to_string(), by_part);

        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = "ABC-123".to_string();
        message.is_from_me = true;

        let mut out = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut out)
            .unwrap();

        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        let tapbacks = value["tapbacks"].as_array().unwrap();
        assert_eq!(tapbacks.len(), 1);
        assert_eq!(tapbacks[0]["kind"], "emoji");
        assert_eq!(tapbacks[0]["emoji"], "🎉");
        assert_eq!(tapbacks[0]["part_index"], 1);
        assert_eq!(tapbacks[0]["is_from_me"], true);
        assert!(tapbacks[0]["sender"].is_null());
    }

    #[test]
    fn reply_emits_thread_metadata() {
        let config = fake_config();
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = "REPLY-1".to_string();
        message.is_from_me = true;
        message.thread_originator_guid = Some("ORIG-1".to_string());
        message.thread_originator_part = Some("2:0:0".to_string());

        let mut out = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut out)
            .unwrap();

        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(value["is_reply"], true);
        assert_eq!(value["thread_originator_guid"], "ORIG-1");
        assert_eq!(value["thread_originator_part"], 2);
    }

    #[test]
    fn thread_originator_message_reports_reply_count() {
        let config = fake_config();
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = "ORIG-1".to_string();
        message.is_from_me = true;
        message.num_replies = 3;

        let mut out = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut out)
            .unwrap();

        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(value["is_reply"], false);
        assert!(value["thread_originator_guid"].is_null());
        assert_eq!(value["num_replies"], 3);
    }

    #[test]
    fn conversation_header_individual_uses_raw_handles() {
        let mut config = fake_config();
        config.participants.insert(10, Name::fake_name("+15551234567"));
        config.real_participants.insert(10, 10);
        let mut participants = std::collections::BTreeSet::new();
        participants.insert(10);
        config.chatroom_participants.insert(0, participants);
        let exporter = JSON::new(&config).unwrap();

        let chat = Chat {
            rowid: 0,
            chat_identifier: "+15551234567".to_string(),
            service_name: Some("iMessage".to_string()),
            display_name: None,
        };

        let header = exporter.conversation_header(&chat).unwrap();
        assert!(header.ends_with('\n'));
        let value: Value = serde_json::from_str(header.trim_end()).unwrap();
        assert_eq!(value["record"], "conversation");
        assert_eq!(value["conversation_type"], "individual");
        assert_eq!(value["chat_identifier"], "+15551234567");
        assert_eq!(value["service"], "iMessage");
        assert!(value["group_title"].is_null());
        let parts = value["participants"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["handle"], "+15551234567");
    }

    #[test]
    fn conversation_header_group_with_title() {
        let mut config = fake_config();
        config.participants.insert(10, Name::fake_name("+15551111111"));
        config.participants.insert(11, Name::fake_name("+15552222222"));
        config.real_participants.insert(10, 10);
        config.real_participants.insert(11, 11);
        let mut participants = std::collections::BTreeSet::new();
        participants.insert(10);
        participants.insert(11);
        config.chatroom_participants.insert(0, participants);
        let exporter = JSON::new(&config).unwrap();

        let chat = Chat {
            rowid: 0,
            chat_identifier: "chat999".to_string(),
            service_name: Some("iMessage".to_string()),
            display_name: Some("Weekend Trip".to_string()),
        };

        let header = exporter.conversation_header(&chat).unwrap();
        let value: Value = serde_json::from_str(header.trim_end()).unwrap();
        assert_eq!(value["conversation_type"], "group");
        assert_eq!(value["group_title"], "Weekend Trip");
        assert_eq!(value["participants"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn message_emits_schema_v3_easy_wins() {
        let config = fake_config();
        let exporter = JSON::new(&config).unwrap();

        let mut message = Config::fake_message();
        message.guid = "V3-1".to_string();
        message.text = Some("hi".to_string());
        message.is_from_me = true;
        message.service = Some("iMessage".to_string());
        message.date_delivered = 674526582885055488;
        message.date_read = 0;
        message.expressive_send_style_id =
            Some("com.apple.MobileSMS.expressivesend.gentle".to_string());

        let mut out = String::new();
        exporter
            .format_message_into(&message, RenderContext::TopLevel, &mut out)
            .unwrap();

        let value: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(value["is_deleted"], false);
        assert_eq!(value["expressive"], "Sent with Gentle");
        assert_eq!(value["service"], "iMessage");
        assert!(value.get("timestamp_read").is_none());
        assert!(value.get("timestamp_delivered").is_some());
        assert!(value.get("parts").is_none() || value["parts"].as_array().unwrap().is_empty());
        assert!(value.get("edits").is_none() || value["edits"].as_array().unwrap().is_empty());
    }

}
