//! Parse SMS Backup & Restore / legacy allsms XML.

use crate::phone::{sanitize_number, to_e164};
use crate::smil::{ordered_smil_refs, part_content_keys, smil_xml_from_parts};
use anyhow::{Context, Result};
use base64::Engine;
use quick_xml::events::Event;
use quick_xml::Reader;
use quick_xml::XmlVersion;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::BufRead;
use std::path::Path;

const INSERT_ADDRESS_TOKEN: &str = "insert-address-token";
const SMS_TYPE_RECEIVED: &str = "1";
const SMS_TYPE_SENT: &str = "2";
const MMS_BOX_SENT: &str = "2";
const MMS_BOX_DRAFT: &str = "3";
const MMS_BOX_OUTBOX: &str = "4";
const MMS_ADDR_FROM: &str = "137";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConvType {
    #[default]
    Individual,
    Group,
}

impl ConvType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Group => "group",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MmsPart {
    pub ct: String,
    pub name: String,
    pub cl: String,
    pub fn_attr: String,
    pub text: String,
    pub data: String,
    pub seq: String,
    /// Every `<part>` attribute as written in the backup.
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct MmsAddr {
    pub address: String,
    pub addr_type: String,
    /// Every `<addr>` attribute as written in the backup.
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AttachmentBlob {
    pub filename: String,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub data: std::sync::Arc<[u8]>,
    pub digest_hex: String,
}

/// Full XML fidelity payload for CSV `xml_fields_json`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum XmlFields {
    #[serde(rename = "sms")]
    Sms {
        attrs: BTreeMap<String, String>,
    },
    #[serde(rename = "mms")]
    Mms {
        attrs: BTreeMap<String, String>,
        parts: Vec<BTreeMap<String, String>>,
        addrs: Vec<BTreeMap<String, String>>,
    },
}

#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub chat_key: String,
    pub conversation_type: ConvType,
    pub group_title: Option<String>,
    /// digits → optional name hint
    pub participant_digits: Vec<(String, Option<String>)>,
    pub timestamp_secs: f64,
    pub is_from_me: bool,
    pub sender_digits: Option<String>,
    pub sender_display_name: Option<String>,
    pub text: String,
    pub subject: String,
    pub attachments: Vec<AttachmentBlob>,
    pub message_kind: &'static str,
    pub date_ms: String,
    pub contact_name: String,
    pub android_type: String,
    pub xml_fields: XmlFields,
}

#[derive(Debug, Default)]
pub struct XmlParseStats {
    pub sms_count: u64,
    pub mms_count: u64,
    pub skipped_invalid_date: u64,
    pub skipped_unknown_address: u64,
    pub skipped_unknown_type: u64,
    pub skipped_empty_participants: u64,
    pub skipped_bad_attachment: u64,
}

fn decode_body(raw: &str) -> String {
    html_escape::decode_html_entities(raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn attr_map(e: &quick_xml::events::BytesStart<'_>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for a in e.attributes().flatten() {
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let val = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map(|v| v.into_owned())
            .unwrap_or_default();
        map.insert(key, val);
    }
    map
}

fn to_btree(m: &HashMap<String, String>) -> BTreeMap<String, String> {
    m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn get<'a>(m: &'a HashMap<String, String>, key: &str) -> &'a str {
    m.get(key).map(|s| s.as_str()).unwrap_or("")
}

fn part_from_attrs(a: &HashMap<String, String>) -> MmsPart {
    MmsPart {
        ct: get(a, "ct").to_string(),
        name: get(a, "name").to_string(),
        cl: get(a, "cl").to_string(),
        fn_attr: get(a, "fn").to_string(),
        text: get(a, "text").to_string(),
        data: get(a, "data").to_string(),
        seq: get(a, "seq").to_string(),
        attrs: to_btree(a),
    }
}

fn addr_from_attrs(a: &HashMap<String, String>) -> MmsAddr {
    MmsAddr {
        address: get(a, "address").to_string(),
        addr_type: get(a, "type").to_string(),
        attrs: to_btree(a),
    }
}

/// CSV-safe part attrs: drop base64 `data`, add `data_len` + `data_sha256` of decoded bytes.
fn part_attrs_for_csv(part: &MmsPart) -> BTreeMap<String, String> {
    let mut m = part.attrs.clone();
    if let Some(data_b64) = m.remove("data") {
        let trimmed = data_b64.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("null") {
            match base64::engine::general_purpose::STANDARD.decode(trimmed) {
                Ok(bytes) => {
                    m.insert("data_len".into(), bytes.len().to_string());
                    m.insert("data_sha256".into(), hex::encode(Sha256::digest(&bytes)));
                }
                Err(_) => {
                    m.insert("data_len".into(), trimmed.len().to_string());
                    m.insert(
                        "data_sha256".into(),
                        hex::encode(Sha256::digest(trimmed.as_bytes())),
                    );
                    m.insert("data_decode_error".into(), "true".into());
                }
            }
        }
    }
    m
}

fn timestamp_secs(date_raw: &str) -> Option<f64> {
    date_raw.parse::<f64>().ok().map(|ms| ms / 1000.0)
}

fn contact_name(attrs: &HashMap<String, String>) -> Option<String> {
    let name = get(attrs, "contact_name");
    let name = if name.is_empty() {
        get(attrs, "name")
    } else {
        name
    };
    let t = name.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("null") {
        None
    } else {
        Some(t.to_string())
    }
}

fn raw_contact_name(attrs: &HashMap<String, String>) -> String {
    let name = get(attrs, "contact_name");
    if !name.is_empty() {
        return name.to_string();
    }
    get(attrs, "name").to_string()
}

fn nullish_to_empty(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("null") {
        String::new()
    } else {
        t.to_string()
    }
}

fn is_valid_received_from(raw: &str, owners: &HashSet<String>) -> bool {
    let stripped = raw.trim();
    if stripped.is_empty() || stripped.eq_ignore_ascii_case(INSERT_ADDRESS_TOKEN) {
        return false;
    }
    match sanitize_number(stripped) {
        Some(sender_num) => !owners.contains(&sender_num),
        None => false,
    }
}

fn mms_participants(address_field: &str, addrs: &[MmsAddr]) -> Vec<String> {
    let mut participants = Vec::new();
    if !address_field.is_empty() {
        for p in address_field.split('~') {
            if !p.trim().is_empty() {
                participants.push(p.trim().to_string());
            }
        }
    }
    for addr in addrs {
        if !addr.address.trim().is_empty() {
            participants.push(addr.address.trim().to_string());
        }
    }
    participants
}

fn mms_sender(
    msg_box: &str,
    addrs: &[MmsAddr],
    participants: &[String],
    owners: &HashSet<String>,
) -> (bool, Option<String>) {
    if msg_box.trim() == MMS_BOX_SENT {
        return (true, None);
    }
    for addr in addrs {
        if addr.addr_type != MMS_ADDR_FROM {
            continue;
        }
        if !is_valid_received_from(&addr.address, owners) {
            continue;
        }
        return (false, sanitize_number(&addr.address));
    }
    // Last resort: first non-owner participant
    for raw in participants {
        if let Some(num) = sanitize_number(raw) {
            if !owners.contains(&num) {
                return (false, Some(num));
            }
        }
    }
    (false, None)
}

fn mms_body_and_attachments(
    parts: &[MmsPart],
    timestamp_ms: f64,
    stats: &mut XmlParseStats,
) -> (String, Vec<AttachmentBlob>) {
    let smil = smil_xml_from_parts(parts);
    let (text_refs, img_refs) = ordered_smil_refs(&smil);

    let mut text_by_key: HashMap<String, String> = HashMap::new();
    for part in parts {
        let ct = part.ct.to_ascii_lowercase();
        if !ct.starts_with("text/") {
            continue;
        }
        let text = decode_body(&part.text);
        if text.is_empty() || text.eq_ignore_ascii_case("null") {
            continue;
        }
        for key in part_content_keys(part) {
            text_by_key.entry(key).or_insert_with(|| text.clone());
        }
    }

    let body = if !text_refs.is_empty() {
        text_refs
            .iter()
            .filter_map(|r| text_by_key.get(r).cloned())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let mut seen = std::collections::HashSet::new();
        let mut parts_out = Vec::new();
        for v in text_by_key.values() {
            if seen.insert(v.clone()) {
                parts_out.push(v.clone());
            }
        }
        parts_out.join("\n")
    };

    let attachments = crate::assets::extract_mms_attachments(parts, timestamp_ms, &img_refs, stats);
    (body, attachments)
}

fn parse_sms(
    attrs: &HashMap<String, String>,
    _owners: &HashSet<String>,
    stats: &mut XmlParseStats,
) -> Option<ParsedMessage> {
    stats.sms_count += 1;
    let date_ms = get(attrs, "date").to_string();
    let ts = match timestamp_secs(&date_ms) {
        Some(t) => t,
        None => {
            stats.skipped_invalid_date += 1;
            return None;
        }
    };
    let addr = match sanitize_number(get(attrs, "address")) {
        Some(a) => a,
        None => {
            stats.skipped_unknown_address += 1;
            return None;
        }
    };
    let typ = get(attrs, "type").trim().to_string();
    let body = decode_body(get(attrs, "body"));
    let hint = contact_name(attrs);
    let contact_name_raw = raw_contact_name(attrs);
    let subject = nullish_to_empty(get(attrs, "subject"));

    let (is_from_me, sender_digits) = match typ.as_str() {
        SMS_TYPE_SENT => (true, None),
        SMS_TYPE_RECEIVED => (false, Some(addr.clone())),
        _ => {
            stats.skipped_unknown_type += 1;
            return None;
        }
    };

    Some(ParsedMessage {
        chat_key: addr.clone(),
        conversation_type: ConvType::Individual,
        group_title: None,
        participant_digits: vec![(addr, hint.clone())],
        timestamp_secs: ts,
        is_from_me,
        sender_digits,
        sender_display_name: if is_from_me { None } else { hint },
        text: body,
        subject,
        attachments: Vec::new(),
        message_kind: "sms",
        date_ms,
        contact_name: contact_name_raw,
        android_type: typ,
        xml_fields: XmlFields::Sms {
            attrs: to_btree(attrs),
        },
    })
}

fn parse_mms(
    attrs: &HashMap<String, String>,
    parts: &[MmsPart],
    addrs: &[MmsAddr],
    owners: &HashSet<String>,
    stats: &mut XmlParseStats,
) -> Option<ParsedMessage> {
    stats.mms_count += 1;
    let date_ms = get(attrs, "date").to_string();
    let ts = match timestamp_secs(&date_ms) {
        Some(t) => t,
        None => {
            stats.skipped_invalid_date += 1;
            return None;
        }
    };
    let participants = mms_participants(get(attrs, "address"), addrs);
    if participants.is_empty() {
        stats.skipped_empty_participants += 1;
        return None;
    }

    let msg_box = get(attrs, "msg_box").trim().to_string();
    // Mirror SMS: only inbox/sent are exported. Draft/outbox are not real chat rows.
    if msg_box == MMS_BOX_DRAFT || msg_box == MMS_BOX_OUTBOX {
        stats.skipped_unknown_type += 1;
        return None;
    }
    let date_ms_f = date_ms.parse::<f64>().unwrap_or(0.0);
    let (body, attachments) = mms_body_and_attachments(parts, date_ms_f, stats);
    let hint = contact_name(attrs);
    let contact_name_raw = raw_contact_name(attrs);
    let subject = nullish_to_empty(get(attrs, "sub"));
    let (is_from_me, sender_digits) = mms_sender(&msg_box, addrs, &participants, owners);

    let xml_fields = XmlFields::Mms {
        attrs: to_btree(attrs),
        parts: parts.iter().map(part_attrs_for_csv).collect(),
        addrs: addrs.iter().map(|a| a.attrs.clone()).collect(),
    };

    let non_owner: Vec<String> = {
        let mut set: Vec<String> = participants
            .iter()
            .filter_map(|p| sanitize_number(p))
            .filter(|p| !owners.contains(p))
            .collect();
        set.sort();
        set.dedup();
        set
    };

    if non_owner.len() <= 1 {
        let counterparty = non_owner
            .first()
            .cloned()
            .or_else(|| sanitize_number(&participants[0]));
        let Some(counterparty) = counterparty else {
            stats.skipped_unknown_address += 1;
            return None;
        };
        return Some(ParsedMessage {
            chat_key: counterparty.clone(),
            conversation_type: ConvType::Individual,
            group_title: None,
            participant_digits: vec![(counterparty, hint.clone())],
            timestamp_secs: ts,
            is_from_me,
            sender_digits,
            sender_display_name: if is_from_me { None } else { hint },
            text: body,
            subject,
            attachments,
            message_kind: "mms",
            date_ms,
            contact_name: contact_name_raw,
            android_type: msg_box,
            xml_fields,
        });
    }

    let others = non_owner;
    let title = if others.len() <= 4 {
        format!(
            "Group: {}",
            others
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "Group: {}, and {} others",
            others[..4]
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", "),
            others.len() - 4
        )
    };
    let chat_key = format!("group-{}", others.join("_"));
    let chat_key = if chat_key.len() > 180 {
        format!(
            "group-{}",
            &hex::encode(Sha256::digest(chat_key.as_bytes()))[..16]
        )
    } else {
        chat_key
    };

    let participant_digits: Vec<(String, Option<String>)> =
        others.into_iter().map(|d| (d, None)).collect();

    Some(ParsedMessage {
        chat_key,
        conversation_type: ConvType::Group,
        group_title: Some(title),
        participant_digits,
        timestamp_secs: ts,
        is_from_me,
        sender_digits,
        sender_display_name: if is_from_me { None } else { hint },
        text: body,
        subject,
        attachments,
        message_kind: "mms",
        date_ms,
        contact_name: contact_name_raw,
        android_type: msg_box,
        xml_fields,
    })
}

/// Stream-parse one XML file into messages.
pub fn parse_xml_file(path: &Path, owners: &HashSet<String>) -> Result<(Vec<ParsedMessage>, XmlParseStats)> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    parse_xml_reader(reader, owners)
}

pub fn parse_xml_reader<R: BufRead>(
    reader: R,
    owners: &HashSet<String>,
) -> Result<(Vec<ParsedMessage>, XmlParseStats)> {
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut stats = XmlParseStats::default();
    let mut messages = Vec::new();
    let mut buf = Vec::new();

    let mut sms_attrs: HashMap<String, String> = HashMap::new();
    let mut mms_attrs: HashMap<String, String> = HashMap::new();
    let mut parts: Vec<MmsPart> = Vec::new();
    let mut addrs: Vec<MmsAddr> = Vec::new();
    let mut current_part = MmsPart::default();
    let mut current_addr = MmsAddr::default();

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                match tag.as_str() {
                    "sms" => {
                        sms_attrs = attr_map(&e);
                    }
                    "mms" => {
                        mms_attrs = attr_map(&e);
                        parts.clear();
                        addrs.clear();
                    }
                    "part" => {
                        current_part = part_from_attrs(&attr_map(&e));
                    }
                    "addr" => {
                        current_addr = addr_from_attrs(&attr_map(&e));
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                match tag.as_str() {
                    "sms" => {
                        let attrs = attr_map(&e);
                        if let Some(msg) = parse_sms(&attrs, owners, &mut stats) {
                            messages.push(msg);
                        }
                    }
                    "part" => {
                        parts.push(part_from_attrs(&attr_map(&e)));
                    }
                    "addr" => {
                        addrs.push(addr_from_attrs(&attr_map(&e)));
                    }
                    "mms" => {
                        let attrs = attr_map(&e);
                        if let Some(msg) = parse_mms(&attrs, &[], &[], owners, &mut stats) {
                            messages.push(msg);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                match tag.as_str() {
                    "sms" => {
                        if let Some(msg) = parse_sms(&sms_attrs, owners, &mut stats) {
                            messages.push(msg);
                        }
                    }
                    "part" => {
                        parts.push(std::mem::take(&mut current_part));
                    }
                    "addr" => {
                        addrs.push(std::mem::take(&mut current_addr));
                    }
                    "mms" => {
                        if let Some(msg) =
                            parse_mms(&mms_attrs, &parts, &addrs, owners, &mut stats)
                        {
                            messages.push(msg);
                        }
                        parts.clear();
                        addrs.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(err).context("XML parse error"),
            _ => {}
        }
        buf.clear();
    }

    Ok((messages, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_owners() -> HashSet<String> {
        let mut owners = HashSet::new();
        owners.insert("5555550100".into());
        owners
    }

    #[test]
    fn parses_sms_sent_received() {
        let xml = br#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="2">
  <sms protocol="0" address="+15555550101" date="1400773261000" type="1" body="hello &amp; hi" contact_name="Sam" />
  <sms protocol="0" address="+15555550101" date="1400773321000" type="2" body="hey" contact_name="Sam" />
</smses>"#;
        let (msgs, stats) = parse_xml_reader(xml.as_slice(), &test_owners()).unwrap();
        assert_eq!(stats.sms_count, 2);
        assert_eq!(msgs.len(), 2);
        assert!(!msgs[0].is_from_me);
        assert_eq!(msgs[0].text, "hello & hi");
        assert!(msgs[1].is_from_me);
        assert_eq!(msgs[0].conversation_type, ConvType::Individual);
        assert_eq!(msgs[0].message_kind, "sms");
    }

    #[test]
    fn preserves_extra_sms_attrs() {
        let xml = br#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="1">
  <sms protocol="0" address="+15555550101" date="1400773261000" type="1" body="hi" read="1" status="-1" service_center="+1555000" />
</smses>"#;
        let (msgs, _) = parse_xml_reader(xml.as_slice(), &test_owners()).unwrap();
        assert_eq!(msgs.len(), 1);
        match &msgs[0].xml_fields {
            XmlFields::Sms { attrs } => {
                assert_eq!(attrs.get("read").map(String::as_str), Some("1"));
                assert_eq!(attrs.get("status").map(String::as_str), Some("-1"));
                assert_eq!(
                    attrs.get("service_center").map(String::as_str),
                    Some("+1555000")
                );
            }
            XmlFields::Mms { .. } => panic!("expected sms"),
        }
        assert_eq!(msgs[0].android_type, "1");
        assert_eq!(msgs[0].date_ms, "1400773261000");
    }

    #[test]
    fn mms_parts_replace_data_with_digest() {
        let xml = br#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="1">
  <mms date="1400773400000" msg_box="1" address="+15555550101">
    <parts>
      <part seq="0" ct="image/jpeg" name="pic.jpg" cl="pic.jpg" chset="null" data="aGVsbG8=" />
    </parts>
    <addrs>
      <addr address="+15555550101" type="137" charset="106" />
      <addr address="+15555550100" type="151" charset="106" />
    </addrs>
  </mms>
</smses>"#;
        let (msgs, _) = parse_xml_reader(xml.as_slice(), &test_owners()).unwrap();
        assert_eq!(msgs.len(), 1);
        match &msgs[0].xml_fields {
            XmlFields::Mms { parts, addrs, .. } => {
                assert_eq!(parts.len(), 1);
                assert!(!parts[0].contains_key("data"));
                assert_eq!(parts[0].get("data_len").map(String::as_str), Some("5"));
                assert!(parts[0].get("data_sha256").is_some());
                assert_eq!(parts[0].get("chset").map(String::as_str), Some("null"));
                assert_eq!(addrs.len(), 2);
                assert_eq!(addrs[0].get("charset").map(String::as_str), Some("106"));
            }
            XmlFields::Sms { .. } => panic!("expected mms"),
        }
    }

    #[test]
    fn skips_draft_and_bad_date() {
        let xml = br#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="5">
  <sms address="+15555550101" date="not-a-date" type="1" body="bad date" />
  <sms address="+15555550101" date="1400773261000" type="3" body="draft" />
  <sms address="+15555550101" date="1400773261000" type="1" body="ok" />
  <mms date="1400773400000" msg_box="3" address="+15555550101">
    <parts><part seq="0" ct="text/plain" text="mms draft" /></parts>
  </mms>
  <mms date="1400773400000" msg_box="4" address="+15555550101">
    <parts><part seq="0" ct="text/plain" text="mms outbox" /></parts>
  </mms>
</smses>"#;
        let (msgs, stats) = parse_xml_reader(xml.as_slice(), &test_owners()).unwrap();
        assert_eq!(stats.sms_count, 3);
        assert_eq!(stats.mms_count, 2);
        assert_eq!(stats.skipped_invalid_date, 1);
        assert_eq!(stats.skipped_unknown_type, 3);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text, "ok");
    }

    #[test]
    fn parses_group_mms() {
        let xml = br#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="1">
  <mms date="1400773400000" msg_box="1" address="+15555550101~+15555550102">
    <parts>
      <part seq="0" ct="application/smil" name="smil.xml" text="&lt;smil&gt;&lt;body&gt;&lt;par&gt;&lt;text src=&quot;text_0.txt&quot;/&gt;&lt;img src=&quot;pic.jpg&quot;/&gt;&lt;/par&gt;&lt;/body&gt;&lt;/smil&gt;" />
      <part seq="1" ct="text/plain" name="text_0.txt" cl="text_0.txt" text="group hi" />
      <part seq="2" ct="image/jpeg" name="pic.jpg" cl="pic.jpg" data="aGVsbG8=" />
    </parts>
    <addrs>
      <addr address="+15555550101" type="137" charset="106" />
      <addr address="+15555550102" type="151" charset="106" />
      <addr address="+15555550100" type="151" charset="106" />
    </addrs>
  </mms>
</smses>"#;
        let (msgs, stats) = parse_xml_reader(xml.as_slice(), &test_owners()).unwrap();
        assert_eq!(stats.mms_count, 1);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].conversation_type, ConvType::Group);
        assert_eq!(msgs[0].text, "group hi");
        assert!(!msgs[0].is_from_me);
        assert_eq!(msgs[0].sender_digits.as_deref(), Some("5555550101"));
        assert_eq!(msgs[0].attachments.len(), 1);
        assert!(msgs[0]
            .group_title
            .as_deref()
            .unwrap_or("")
            .contains("+15555550101"));
    }

    #[test]
    fn owner_phone_controls_mms_direction() {
        let xml = br#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="1">
  <mms date="1400773400000" msg_box="2" address="+15555550101">
    <parts>
      <part seq="0" ct="text/plain" text="sent by me" />
    </parts>
    <addrs>
      <addr address="+15555550100" type="137" charset="106" />
      <addr address="+15555550101" type="151" charset="106" />
    </addrs>
  </mms>
</smses>"#;
        let (msgs, _) = parse_xml_reader(xml.as_slice(), &test_owners()).unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].is_from_me);
        assert_eq!(msgs[0].conversation_type, ConvType::Individual);
    }
}
