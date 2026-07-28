//! Stream messages → [`MailMessage`] → per-conversation `.eml` / `.mbox` files.

use std::collections::HashMap;

use imessage_database::{
    message_types::{
        handwriting::HandwrittenMessage,
        variants::{Announcement, Tapback, TapbackAction, Variant},
    },
    tables::{
        attachment::Attachment,
        chat::Chat,
        messages::{
            Message,
            models::{GroupAction, Service},
        },
        table::{Table, ME, YOU},
    },
    util::dates::TIMESTAMP_FACTOR,
};
use message_exporters_core::OutputFormat;
use message_mail::{
    append_message_mbox, conversation_stem, write_message_file, Direction as MailDirection,
    MailAttachment, MailMessage, Participant,
};

use crate::{
    attachments::load_attachment_bytes,
    body::{apply_body, referenced_attachment_indices},
    error::RuntimeError,
    fields::{
        balloon_kind_label, balloon_summary, build_balloon_value, build_edit_records,
        build_part_records, expressive_label, parse_thread_part, shared_location_label,
        sticker_extras, transcription_for_attachment, TapbackCell,
    },
    session::MailSession,
};

const EXPORT_SOURCE: &str = "imessage";
const EXPORT_TOOL: &str = "imessage-mail-exporter";

/// Stream chat.db into per-conversation `.eml` folders or `.mbox` files.
pub fn run_export(session: &MailSession) -> Result<(), RuntimeError> {
    let format = session.options.output_format;
    eprintln!(
        "Exporting to {} as {}...",
        session.options.export_path.display(),
        format.as_str()
    );

    let mut seq_by_stem: HashMap<String, u32> = HashMap::new();
    let mut current_message_row = -1;
    let mut current_message = 0u64;
    let mut failures: u64 = 0;
    let total_messages =
        Message::get_count(session.data_source.db(), &session.options.query_context)?;

    let mut statement =
        Message::stream_rows(session.data_source.db(), &session.options.query_context)?;

    for message in Message::rows(&mut statement, [])? {
        let mut msg = message?;

        if msg.rowid == current_message_row {
            current_message += 1;
            continue;
        }
        current_message_row = msg.rowid;

        // Poll vote/update noise — keep skipping (same as CSV/HTML export focus).
        if !msg.is_edited() && (msg.is_poll_vote() || msg.is_poll_update()) {
            current_message += 1;
            continue;
        }

        apply_body(&mut msg, session.data_source.db());

        if msg.is_poll_vote() || msg.is_poll_update() {
            current_message += 1;
            continue;
        }

        match write_one(session, &mut seq_by_stem, &msg) {
            Ok(()) => {}
            Err(why) => {
                failures += 1;
                eprintln!(
                    "Skipping message (rowid={}, guid={}): {}",
                    msg.rowid, msg.guid, why
                );
            }
        }
        current_message += 1;
        if current_message.is_multiple_of(500) {
            eprintln!("  …{current_message}/{total_messages}");
        }
    }

    if failures > 0 {
        eprintln!("{failures} messages skipped due to formatting errors.");
    }

    Ok(())
}

fn write_one(
    session: &MailSession,
    seq_by_stem: &mut HashMap<String, u32>,
    message: &Message,
) -> Result<(), RuntimeError> {
    let mail = build_mail_message(session, message)?;
    match session.options.output_format {
        OutputFormat::Eml => {
            let stem = session.conversation_stem(message);
            let conv_dir = session.options.export_path.join(&stem);
            let seq = seq_by_stem.entry(stem).or_insert(0);
            *seq += 1;
            write_message_file(&conv_dir, *seq, &mail)
                .map_err(|e| RuntimeError::InvalidOptions(format!("write eml: {e:#}")))?;
        }
        OutputFormat::Mbox => {
            let stem = conversation_stem(&mail);
            let mbox_path = session.options.export_path.join(format!("{stem}.mbox"));
            *seq_by_stem.entry(stem).or_insert(0) += 1;
            append_message_mbox(&mbox_path, &mail)
                .map_err(|e| RuntimeError::InvalidOptions(format!("write mbox: {e:#}")))?;
        }
        OutputFormat::Csv => {
            return Err(RuntimeError::InvalidOptions(
                "imessage-mail-exporter does not write CSV".to_string(),
            ));
        }
    }
    Ok(())
}

fn timestamp_unix_ms(message: &Message, offset: i64) -> i64 {
    if let Ok(dt) = message.date(offset) {
        return dt.timestamp_millis();
    }
    let stamp = message.date;
    let seconds_since_2001 = if stamp >= 1_000_000_000_000 {
        stamp / TIMESTAMP_FACTOR
    } else {
        stamp
    };
    (seconds_since_2001 + offset).saturating_mul(1000)
}

fn raw_handle(session: &MailSession, handle_id: i32) -> Option<String> {
    session
        .resolve_participant(handle_id)
        .map(|name| name.details.clone())
}

fn display_name_for(session: &MailSession, handle_id: i32) -> Option<String> {
    session.resolve_participant(handle_id).map(|name| {
        if name.full.is_empty() {
            name.details.clone()
        } else {
            name.full.clone()
        }
    })
}

fn participants_for(session: &MailSession, chatroom: &Chat) -> (Vec<Participant>, &'static str) {
    let mut records = Vec::new();
    let mut count = 0;
    if let Some(handles) = session.chatroom_participants.get(&chatroom.rowid) {
        count = handles.len();
        for handle_id in handles {
            let name = session.resolve_participant(*handle_id);
            let (handle, display_name) = match name {
                Some(n) => (
                    n.details.clone(),
                    if n.full.is_empty() {
                        None
                    } else {
                        Some(n.full.clone())
                    },
                ),
                None => (String::new(), None),
            };
            if !handle.is_empty() {
                records.push(Participant {
                    handle,
                    display_name,
                });
            }
        }
    }
    let conversation_type = if count > 1 { "group" } else { "individual" };
    (records, conversation_type)
}

fn announcement_text(session: &MailSession, msg: &Message) -> Option<String> {
    let announcement = msg.get_announcement()?;
    let mut who = session.who(msg.handle_id, msg.is_from_me(), &msg.destination_caller_id);
    if who == ME {
        who = YOU;
    }
    let participant_name = match &announcement {
        Announcement::GroupAction(
            GroupAction::ParticipantAdded(handle) | GroupAction::ParticipantRemoved(handle),
        ) => session.who(Some(*handle), false, &msg.destination_caller_id),
        _ => "someone",
    };

    let body = match &announcement {
        Announcement::AudioMessageKept => "kept an audio message.".to_string(),
        Announcement::FullyUnsent => "unsent a message!".to_string(),
        Announcement::Unknown(num) => format!("performed unknown action {num}."),
        Announcement::GroupAction(group) => match group {
            GroupAction::ParticipantAdded(_) => {
                format!("added {participant_name} to the conversation.")
            }
            GroupAction::ParticipantRemoved(_) => {
                format!("removed {participant_name} from the conversation.")
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
    Some(format!("{who} {body}"))
}

fn owner_display_name(session: &MailSession, message: &Message) -> Option<String> {
    if session.options.use_caller_id {
        message
            .destination_caller_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| Some(ME.to_string()))
    } else {
        None
    }
}

fn tapback_human_line(kind: &str, emoji: Option<&str>, action: &str) -> String {
    if action == "remove" {
        return match kind {
            "loved" => "Removed Heart".into(),
            "liked" => "Removed Like".into(),
            "disliked" => "Removed Dislike".into(),
            "laughed" => "Removed Laugh".into(),
            "emphasized" => "Removed Exclamation".into(),
            "questioned" => "Removed Question Mark".into(),
            "emoji" => format!("Removed {}", emoji.unwrap_or("emoji")),
            "sticker" => "Removed Sticker".into(),
            other => format!("Removed {other}"),
        };
    }
    match kind {
        "loved" => "Loved a message".into(),
        "liked" => "Liked a message".into(),
        "disliked" => "Disliked a message".into(),
        "laughed" => "Laughed at a message".into(),
        "emphasized" => "Emphasized a message".into(),
        "questioned" => "Questioned a message".into(),
        "emoji" => format!("{} reacted", emoji.unwrap_or("Emoji")),
        "sticker" => "Reacted with a sticker".into(),
        other => format!("{other} reaction"),
    }
}

fn build_parent_tapbacks_json(session: &MailSession, message: &Message) -> Option<String> {
    let parts = session.tapbacks.get(&message.guid)?;
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
                (
                    None,
                    Some(
                        owner_display_name(session, tapback)
                            .unwrap_or_else(|| ME.to_string()),
                    ),
                )
            } else if let Some(handle_id) = tapback.handle_id {
                (
                    raw_handle(session, handle_id),
                    display_name_for(session, handle_id),
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
    if sortable.is_empty() {
        return None;
    }
    sortable.sort_by_key(|(part, date, rowid, _)| (*part, *date, *rowid));
    let cells: Vec<_> = sortable.into_iter().map(|(_, _, _, c)| c).collect();
    serde_json::to_string(&cells).ok()
}

fn try_handwriting_svg(session: &MailSession, message: &Message) -> Option<MailAttachment> {
    if !message.is_handwriting() {
        return None;
    }
    let payload = message.raw_payload_data(session.data_source.db())?;
    let hw = HandwrittenMessage::from_payload(&payload).ok()?;
    let svg = hw.render_svg();
    Some(MailAttachment {
        bytes: svg.into_bytes(),
        original_name: Some(format!("{}.svg", message.guid)),
        mime_type: Some("image/svg+xml".into()),
        digest_sha256: None,
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
    })
}

fn build_mail_message(
    session: &MailSession,
    message: &Message,
) -> Result<MailMessage, RuntimeError> {
    let (chat_identifier, conversation_type, group_title, participants) =
        match session.conversation(message) {
            Some((chatroom, _)) => {
                let (participants, conversation_type) = participants_for(session, chatroom);
                (
                    chatroom.chat_identifier.clone(),
                    conversation_type.to_string(),
                    chatroom
                        .display_name()
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .map(str::to_string),
                    participants,
                )
            }
            None => (
                String::new(),
                "individual".to_string(),
                None,
                Vec::new(),
            ),
        };

    let is_from_me = message.is_from_me();
    let (sender_handle, sender_display_name) = if is_from_me {
        (None, None)
    } else if let Some(handle_id) = message.handle_id {
        (
            raw_handle(session, handle_id),
            display_name_for(session, handle_id),
        )
    } else {
        (None, None)
    };

    let service = match message.service() {
        Service::Unknown => String::new(),
        other => other.to_string(),
    };

    let mut attachments = Attachment::from_message(session.data_source.db(), message)?;
    let referenced = referenced_attachment_indices(message, &attachments);
    let emitted_index: HashMap<usize, usize> = referenced
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

    let mut mail_attachments = Vec::new();
    for &idx in &referenced {
        let attachment = &mut attachments[idx];
        let bytes = load_attachment_bytes(session, attachment)?;
        let transcription = transcription_for_attachment(message, attachment);
        let (_prompt, sticker_effect) = sticker_extras(
            attachment,
            &session.options.platform,
            session.options.db_path.as_path(),
            session.options.attachment_root.as_deref(),
        );
        mail_attachments.push(MailAttachment {
            bytes,
            original_name: attachment.transfer_name.clone(),
            mime_type: attachment.mime_type.clone(),
            digest_sha256: None,
            is_sticker: attachment.is_sticker,
            transcription,
            sticker_effect,
        });
    }

    if let Some(svg) = try_handwriting_svg(session, message) {
        mail_attachments.push(svg);
    }

    let send_effect = expressive_label(message.get_expressive());
    let shared_location = message
        .shared_location_kind()
        .map(shared_location_label)
        .map(str::to_string);

    let app_value = build_balloon_value(session.data_source.db(), message);
    let balloon_kind = app_value.as_ref().and_then(balloon_kind_label);
    let balloon_bundle_id = message.balloon_bundle_id.clone();

    let edits = message
        .edited_parts
        .as_ref()
        .map(|edited| build_edit_records(edited, &session.offset))
        .unwrap_or_default();

    // --- Tapback path ---
    let message_kind;
    let mut text;
    let mut announcement = None;
    let mut associated_guid = None;
    let mut associated_part = None;
    let mut tapback_kind = None;
    let mut tapback_emoji = None;
    let mut tapback_action = None;
    let mut in_reply_to_guid = None;
    let mut is_reply = false;
    let mut thread_originator_part = None;

    if let Variant::Tapback(_, action, kind) = message.variant() {
        let (kind_s, emoji) = match kind {
            Tapback::Loved => ("loved", None),
            Tapback::Liked => ("liked", None),
            Tapback::Disliked => ("disliked", None),
            Tapback::Laughed => ("laughed", None),
            Tapback::Emphasized => ("emphasized", None),
            Tapback::Questioned => ("questioned", None),
            Tapback::Emoji(e) => ("emoji", e.map(str::to_string)),
            Tapback::Sticker => ("sticker", None),
        };
        let action_s = match action {
            TapbackAction::Added => "add",
            TapbackAction::Removed => "remove",
        };
        message_kind = if matches!(kind, Tapback::Sticker) {
            "sticker_tapback".to_string()
        } else {
            "tapback".to_string()
        };
        if let Some((part, guid)) = message.clean_associated_guid() {
            associated_guid = Some(guid.to_string());
            associated_part = Some(part as u32);
            in_reply_to_guid = Some(guid.to_string());
        }
        tapback_kind = Some(kind_s.to_string());
        tapback_emoji = emoji;
        tapback_action = Some(action_s.to_string());
        text = tapback_human_line(kind_s, tapback_emoji.as_deref(), action_s);
    } else if message.is_shareplay() {
        message_kind = "announcement".to_string();
        text = "SharePlay Message Ended".to_string();
        announcement = Some(text.clone());
    } else if message.is_announcement() {
        message_kind = "announcement".to_string();
        text = announcement_text(session, message).unwrap_or_default();
        announcement = Some(text.clone());
    } else if shared_location.is_some() {
        message_kind = "location_share".to_string();
        text = message.text.clone().unwrap_or_else(|| {
            format!(
                "Shared location {}",
                shared_location.as_deref().unwrap_or("started")
            )
        });
    } else if app_value.is_some() {
        message_kind = "balloon".to_string();
        text = app_value
            .as_ref()
            .map(|v| balloon_summary(v, message.text.as_deref()))
            .unwrap_or_default();
    } else if service.eq_ignore_ascii_case("imessage") {
        message_kind = "imessage".to_string();
        text = message.text.clone().unwrap_or_default();
    } else if !mail_attachments.is_empty() {
        message_kind = "mms".to_string();
        text = message.text.clone().unwrap_or_default();
    } else {
        message_kind = "sms".to_string();
        text = message.text.clone().unwrap_or_default();
    }

    // Replies (non-tapback): own message + thread headers.
    if !message.is_tapback() && message.is_reply() {
        is_reply = true;
        if let Some(guid) = message.thread_originator_guid.clone() {
            in_reply_to_guid = Some(guid);
        }
        thread_originator_part = message
            .thread_originator_part
            .as_deref()
            .and_then(parse_thread_part);
    }

    if let Some(effect) = send_effect.as_deref() {
        if text.is_empty() {
            text = effect.to_string();
        } else if !text.contains(effect) {
            text = format!("{text}\n\n{effect}");
        }
    }

    let read_receipt_rfc3339 = message
        .date_read(session.offset)
        .ok()
        .map(|d| d.to_rfc3339());

    let num_replies = if message.num_replies > 0 {
        Some(message.num_replies as u32)
    } else {
        None
    };

    let parts_json = if parts.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&parts).unwrap_or_else(|_| "[]".into()))
    };
    let edits_json = if edits.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&edits).unwrap_or_else(|_| "[]".into()))
    };
    let app_json = app_value.map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "null".into()));
    let tapbacks_json = if message.is_tapback() {
        None
    } else {
        build_parent_tapbacks_json(session, message)
    };

    let owner_handle = message
        .destination_caller_id
        .clone()
        .unwrap_or_default();

    Ok(MailMessage {
        chat_identifier,
        conversation_type,
        group_title,
        participants,
        guid: message.guid.clone(),
        timestamp_unix_ms: timestamp_unix_ms(message, session.offset),
        direction: if is_from_me {
            MailDirection::Outgoing
        } else {
            MailDirection::Incoming
        },
        service,
        message_kind,
        sender_handle,
        sender_display_name,
        owner_handle,
        owner_display_name: owner_display_name(session, message),
        subject: message.subject.clone().filter(|s| !s.is_empty()),
        text,
        android_type: None,
        source_fields_json: None,
        export_source: EXPORT_SOURCE.into(),
        export_tool: EXPORT_TOOL.into(),
        export_tool_version: env!("CARGO_PKG_VERSION").into(),
        attachments: mail_attachments,
        is_reply,
        in_reply_to_guid,
        thread_originator_part,
        num_replies,
        is_deleted: message.is_deleted(),
        send_effect,
        shared_location,
        announcement,
        read_receipt_rfc3339,
        parts_json,
        edits_json,
        app_json,
        balloon_bundle_id,
        balloon_kind,
        tapbacks_json,
        associated_guid,
        associated_part,
        tapback_kind,
        tapback_emoji,
        tapback_action,
    })
}
