//! Apply shared contact book / name mapping to SMS Backup+ messages.

use crate::types::ParsedMessage;
use message_contacts::{normalize_name_key, ContactsBook, NameMapping};

/// Fill empty peer phone from the contacts book using current `name_hint`.
///
/// Returns `Some((display_name, phone))` when a phone fill happened.
/// Call [`apply_name_mapping`] first so aliases resolve to the contacts CSV name.
pub(crate) fn fill_unknown_phone(
    msg: &mut ParsedMessage,
    book: &ContactsBook,
) -> Option<(String, String)> {
    if !msg.chat_key.is_empty() {
        return None;
    }
    let display = msg
        .name_hint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let phone = book.lookup_phone_by_name(&display)?;
    msg.chat_key = phone.clone();
    if !msg.is_from_me {
        msg.sender_digits = Some(phone.clone());
    }
    msg.participant_digits = vec![(phone.clone(), Some(display.clone()))];
    Some((display, phone))
}

/// Rewrite `name_hint` when the EML name appears as `incorrect_name` in the mapping.
///
/// Returns `Some((from, to))` when the hint changed.
pub(crate) fn apply_name_mapping(
    msg: &mut ParsedMessage,
    mapping: &NameMapping,
) -> Option<(String, String)> {
    let raw = msg
        .name_hint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let correct = mapping.correct_name(&raw)?.to_string();
    if normalize_name_key(&raw) == normalize_name_key(&correct) {
        return None;
    }
    msg.name_hint = Some(correct.clone());
    for (_digits, name) in &mut msg.participant_digits {
        if name
            .as_deref()
            .is_some_and(|n| normalize_name_key(n) == normalize_name_key(&raw))
        {
            *name = Some(correct.clone());
        }
    }
    Some((raw, correct))
}

/// Fill blank/unknown display names from phone→name when the peer phone is known.
pub(crate) fn enrich_display_names(msg: &mut ParsedMessage, book: &ContactsBook) {
    if let Some(ref digits) = msg.sender_digits {
        if let Some(name) = book.enrich_display_name(digits, msg.name_hint.as_deref().unwrap_or(""))
        {
            msg.name_hint = Some(name);
        }
    }
    if !msg.chat_key.is_empty() {
        if let Some(name) = book.enrich_display_name(&msg.chat_key, msg.name_hint.as_deref().unwrap_or(""))
        {
            msg.name_hint = Some(name);
        }
    }
    for (digits, name) in &mut msg.participant_digits {
        let current = name.as_deref().unwrap_or("");
        if let Some(resolved) = book.enrich_display_name(digits, current) {
            *name = Some(resolved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_contacts::ContactsBook;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_csv(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn name_mapping_then_phone_fill() {
        let dir = tempfile::tempdir().unwrap();
        let contacts = write_csv(
            &dir,
            "contacts.csv",
            "First Name,Last Name,Mobile Phone\n\
Jordan,Alias,15555550144\n",
        );
        let mapping_path = write_csv(
            &dir,
            "mapping.csv",
            "correct_name,incorrect_name\n\
Jordan Alias,Jordan Alias (SKIP)\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let mapping = NameMapping::load(&mapping_path).unwrap();

        let mut msg = ParsedMessage {
            chat_key: String::new(),
            conversation_type: "individual".into(),
            group_title: None,
            participant_digits: vec![],
            timestamp_secs: 1.0,
            is_from_me: false,
            sender_digits: None,
            text: "hi".into(),
            attachments: vec![],
            name_hint: Some("Jordan Alias (SKIP)".into()),
            smssync_id: None,
            source_kind: "flat".into(),
            android_type: String::new(),
            eml_path: String::new(),
        };
        let mapped = apply_name_mapping(&mut msg, &mapping).unwrap();
        assert_eq!(mapped.1, "Jordan Alias");
        let hit = fill_unknown_phone(&mut msg, &book).unwrap();
        assert_eq!(hit.1, "5555550144");
        assert_eq!(msg.chat_key, "5555550144");
    }
}
