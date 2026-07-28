//! Stream messages → [`MailMessage`] → per-conversation `.eml` files.

use std::collections::HashMap;

use imessage_database::{
    message_types::variants::Announcement,
    tables::{
        attachment::Attachment,
        chat::Chat,
        messages::{
            Message,
            models::{GroupAction, Service},
        },
        table::{Table, YOU},
    },
    util::dates::TIMESTAMP_FACTOR,
};
use message_mail::{
    write_message_file, Direction as MailDirection, MailAttachment, MailMessage, Participant,
};

use crate::{
    attachments::load_attachment_bytes,
    body::{apply_body, referenced_attachment_indices},
    error::RuntimeError,
    session::MailSession,
};

const EXPORT_SOURCE: &str = "imessage";
const EXPORT_TOOL: &str = "imessage-mail-exporter";

/// Stream chat.db into per-conversation `.eml` folders.
pub fn run_export(session: &MailSession) -> Result<(), RuntimeError> {
    eprintln!(
        "Exporting to {} as eml...",
        session.options.export_path.display()
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

        if !msg.is_edited() && (msg.is_tapback() || msg.is_poll_vote() || msg.is_poll_update()) {
            current_message += 1;
            continue;
        }

        apply_body(&mut msg, session.data_source.db());

        if msg.is_tapback() || msg.is_poll_vote() || msg.is_poll_update() {
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
    let stem = session.conversation_stem(message);
    let conv_dir = session.options.export_path.join(&stem);
    let seq = seq_by_stem.entry(stem).or_insert(0);
    *seq += 1;
    write_message_file(&conv_dir, *seq, &mail)
        .map_err(|e| RuntimeError::InvalidOptions(format!("write eml: {e:#}")))?;
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
    if who == imessage_database::tables::table::ME {
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

    let attachments = Attachment::from_message(session.data_source.db(), message)?;
    let referenced = referenced_attachment_indices(message, &attachments);
    let mut mail_attachments = Vec::new();
    for &idx in &referenced {
        let attachment = &attachments[idx];
        let bytes = load_attachment_bytes(session, attachment)?;
        mail_attachments.push(MailAttachment {
            bytes,
            original_name: attachment.transfer_name.clone(),
            mime_type: attachment.mime_type.clone(),
            digest_sha256: None,
            is_sticker: attachment.is_sticker,
        });
    }

    let message_kind = if service.eq_ignore_ascii_case("imessage") {
        "imessage".to_string()
    } else if !mail_attachments.is_empty() {
        "mms".to_string()
    } else {
        "sms".to_string()
    };

    let text = if message.is_announcement() {
        announcement_text(session, message).unwrap_or_default()
    } else {
        message.text.clone().unwrap_or_default()
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
        subject: message.subject.clone().filter(|s| !s.is_empty()),
        text,
        android_type: None,
        source_fields_json: None,
        export_source: EXPORT_SOURCE.into(),
        export_tool: EXPORT_TOOL.into(),
        export_tool_version: env!("CARGO_PKG_VERSION").into(),
        attachments: mail_attachments,
    })
}
