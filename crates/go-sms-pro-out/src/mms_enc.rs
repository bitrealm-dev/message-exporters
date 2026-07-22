//! Decode-oriented WAP-209 / WAP-230 helpers for MMS binary PDUs.
//!
//! Algorithm reference (not a dependency / not copied): OMA WAP-209 MMS Encapsulation,
//! WAP-230 WSP, and the decode path concepts in python-messaging's `messaging.mms`.
//! GO SMS Pro backups often store a partial header fragment (From/To + named text)
//! rather than a full `m-retrieve-conf` PDU; this module handles both.

use std::collections::HashMap;

/// Well-known MMS field names (WAP-209 table 8). Stored as short-integer values
/// (MSB already cleared); on the wire they appear as `value | 0x80`.
const MMS_FROM: u8 = 0x09;
const MMS_TO: u8 = 0x17;
const MMS_CC: u8 = 0x02;
const MMS_CONTENT_TYPE: u8 = 0x04;
const MMS_CONTENT_LOCATION: u8 = 0x03;
const MMS_MESSAGE_TYPE: u8 = 0x0c;
const MMS_DATE: u8 = 0x05;

/// Subset of WSP well-known content types (WAP-230 table 40) used for attachments.
const WELL_KNOWN_CONTENT_TYPES: &[&str] = &[
    "*/*",
    "text/*",
    "text/html",
    "text/plain",
    "multipart/*",
    "multipart/mixed",
    "multipart/form-data",
    "multipart/byteranges",
    "multipart/alternative",
    "application/*",
    "application/java-vm",
    "application/x-www-form-urlencoded",
    "application/hdmlc",
    "application/vnd.wap.wmlc",
    "application/vnd.wap.wmlscriptc",
    "application/vnd.wap.wta-eventc",
    "application/vnd.wap.uaprof",
    "application/vnd.wap.wtls-ca-certificate",
    "application/vnd.wap.wtls-user-certificate",
    "application/x-x509-ca-cert",
    "application/x-x509-user-cert",
    "image/*",
    "image/gif",
    "image/jpeg",
    "image/tiff",
    "image/png",
    "image/vnd.wap.wbmp",
    "application/vnd.wap.multipart.*",
    "application/vnd.wap.multipart.mixed",
    "application/vnd.wap.multipart.form-data",
    "application/vnd.wap.multipart.byteranges",
    "application/vnd.wap.multipart.alternative",
    "application/xml",
    "text/xml",
    "application/vnd.wap.wbxml",
    "application/x-x968-cross-cert",
    "application/x-x968-ca-cert",
    "application/x-x968-user-cert",
    "text/vnd.wap.si",
    "application/vnd.wap.sic",
    "text/vnd.wap.sl",
    "application/vnd.wap.slc",
    "text/vnd.wap.co",
    "application/vnd.wap.coc",
    "application/vnd.wap.multipart.related",
    "application/vnd.wap.sia",
    "text/vnd.wap.connectivity-xml",
    "application/vnd.wap.connectivity-wbxml",
    "application/pkcs7-mime",
    "application/vnd.wap.hashed-certificate",
    "application/vnd.wap.signed-certificate",
    "application/vnd.wap.cert-response",
    "application/xhtml+xml",
    "application/wml+xml",
    "text/css",
    "application/vnd.wap.mms-message",
    "application/vnd.wap.rollover-certificate",
    "application/vnd.wap.locc+wbxml",
    "application/vnd.wap.loc+xml",
    "application/vnd.syncml.dm+wbxml",
    "application/vnd.syncml.dm+xml",
    "application/vnd.syncml.notification",
    "application/vnd.wap.xhtml+xml",
    "application/vnd.wv.csp.cir",
    "application/vnd.oma.dd+xml",
    "application/vnd.oma.drm.message",
    "application/vnd.oma.drm.content",
    "application/vnd.oma.drm.rights+xml",
    "application/vnd.oma.drm.rights+wbxml",
];

#[derive(Debug, Clone)]
pub struct MmsPart {
    pub content_type: String,
    pub content_location: Option<String>,
    pub data: Vec<u8>,
}

/// Content-Location (`0x8e`) named payload from GO SMS Pro fragments.
#[derive(Debug, Clone)]
pub struct NamedPart {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct StructuredMms {
    pub message_type: Option<String>,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub content_type: Option<String>,
    pub date_unix: Option<u64>,
    pub parts: Vec<MmsPart>,
    pub named_parts: Vec<NamedPart>,
}

impl StructuredMms {
    pub fn is_useful(&self) -> bool {
        self.from.is_some()
            || !self.to.is_empty()
            || !self.cc.is_empty()
            || !self.parts.is_empty()
            || !self.named_parts.is_empty()
    }

    pub fn address_strings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(from) = &self.from {
            out.push(from.clone());
        }
        out.extend(self.to.iter().cloned());
        out.extend(self.cc.iter().cloned());
        out
    }
}

#[derive(Debug)]
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Result<u8, ()> {
        let b = self.peek().ok_or(())?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ()> {
        if self.remaining() < n {
            return Err(());
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
}

fn decode_uint_var(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    let mut value = 0u64;
    for _ in 0..5 {
        let byte = cur.next_byte()?;
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(())
}

fn decode_value_length(cur: &mut Cursor<'_>) -> Result<usize, ()> {
    let byte = cur.peek().ok_or(())?;
    if byte <= 30 {
        cur.next_byte()?;
        Ok(usize::from(byte))
    } else if byte == 31 {
        cur.next_byte()?;
        Ok(decode_uint_var(cur)? as usize)
    } else {
        Err(())
    }
}

fn decode_text_string(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let start = cur.pos;
    while cur.pos < cur.data.len() && cur.data[cur.pos] != 0 {
        cur.pos += 1;
    }
    if cur.pos >= cur.data.len() {
        return Err(());
    }
    let s = String::from_utf8_lossy(&cur.data[start..cur.pos]).into_owned();
    cur.pos += 1; // skip NUL
    Ok(s)
}

fn decode_short_integer(cur: &mut Cursor<'_>) -> Result<u8, ()> {
    let byte = cur.peek().ok_or(())?;
    if byte & 0x80 == 0 {
        return Err(());
    }
    cur.next_byte()?;
    Ok(byte & 0x7f)
}

fn decode_long_integer(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    let len = cur.peek().ok_or(())?;
    if len == 0 || len > 30 {
        return Err(());
    }
    cur.next_byte()?;
    let bytes = cur.take(usize::from(len))?;
    let mut value = 0u64;
    for b in bytes {
        value = (value << 8) | u64::from(*b);
    }
    Ok(value)
}

fn decode_integer_value(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    if let Ok(v) = decode_short_integer(cur) {
        return Ok(u64::from(v));
    }
    decode_long_integer(cur)
}

fn decode_well_known_charset(cur: &mut Cursor<'_>) -> Result<(), ()> {
    // Consume charset token; we treat text as UTF-8 / lossy UTF-8 later.
    let _ = decode_integer_value(cur)?;
    Ok(())
}

/// Encoded-string-value = Text-string | Value-length Char-set Text-string
///
/// GO SMS Pro PDUs often declare a Value-length that overlaps the next MMS
/// short-integer header. Read text until NUL or a high-bit header byte; do not
/// blindly consume through `end` when that would swallow the next field.
fn decode_encoded_string_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let saved = cur.pos;
    if let Ok(len) = decode_value_length(cur) {
        let end = cur.pos.checked_add(len).ok_or(())?;
        if end > cur.data.len() {
            cur.pos = saved;
            return Err(());
        }
        // Charset is optional in some vendor dumps; try, else treat remainder as text.
        let after_len = cur.pos;
        if decode_well_known_charset(cur).is_err() {
            cur.pos = after_len;
        }
        if cur.pos > end {
            cur.pos = saved;
            return Err(());
        }
        let start = cur.pos;
        while cur.pos < end {
            let b = cur.data[cur.pos];
            if b == 0 || b & 0x80 != 0 {
                break;
            }
            cur.pos += 1;
        }
        let text = String::from_utf8_lossy(&cur.data[start..cur.pos])
            .trim_end_matches('\0')
            .to_string();
        if cur.pos < end && cur.data[cur.pos] == 0 {
            cur.pos += 1;
        }
        // Skip only interior padding; leave a following short-integer header in place.
        while cur.pos < end && cur.data[cur.pos] == 0 {
            cur.pos += 1;
        }
        if cur.pos < end && cur.data[cur.pos] & 0x80 == 0 {
            cur.pos = end;
        }
        if !text.is_empty() {
            return Ok(text);
        }
        cur.pos = saved;
        return Err(());
    }
    cur.pos = saved;
    decode_text_string(cur)
}

/// From-value = Value-length (Address-present-token Encoded-string-value | Insert-address-token)
fn decode_from_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let len = decode_value_length(cur)?;
    let end = cur.pos + len;
    if end > cur.data.len() {
        return Err(());
    }
    let token = cur.next_byte()?;
    if token == 0x81 {
        // Insert-address-token
        cur.pos = end;
        return Ok(String::new());
    }
    if token != 0x80 {
        return Err(());
    }
    let addr = decode_encoded_string_value(cur)?;
    // Same Value-length overshoot quirk as Encoded-string-value.
    while cur.pos < end && cur.data[cur.pos] == 0 {
        cur.pos += 1;
    }
    if cur.pos < end && cur.data[cur.pos] & 0x80 != 0 {
        return Ok(addr);
    }
    cur.pos = end.max(cur.pos);
    Ok(addr)
}

fn decode_date_value(cur: &mut Cursor<'_>) -> Result<u64, ()> {
    decode_long_integer(cur)
}

fn decode_message_type_value(cur: &mut Cursor<'_>) -> Result<String, ()> {
    let v = decode_short_integer(cur)?;
    let name = match v {
        0x00 => "m-send-req",
        0x01 => "m-send-conf",
        0x02 => "m-notification-ind",
        0x03 => "m-notifyresp-ind",
        0x04 => "m-retrieve-conf",
        0x05 => "m-acknowledge-ind",
        0x06 => "m-delivery-ind",
        0x07 => "m-read-rec-ind",
        0x08 => "m-read-orig-ind",
        0x09 => "m-forward-req",
        0x0a => "m-forward-conf",
        other => return Ok(format!("unknown-0x{other:02x}")),
    };
    Ok(name.to_string())
}

fn well_known_content_type(id: u64) -> Option<&'static str> {
    WELL_KNOWN_CONTENT_TYPES.get(id as usize).copied()
}

fn decode_constrained_media(cur: &mut Cursor<'_>) -> Result<String, ()> {
    if let Ok(id) = decode_short_integer(cur) {
        return well_known_content_type(u64::from(id))
            .map(str::to_string)
            .ok_or(());
    }
    decode_text_string(cur)
}

fn decode_content_type_value(cur: &mut Cursor<'_>) -> Result<(String, HashMap<String, String>), ()> {
    let saved = cur.pos;
    if let Ok(ct) = decode_constrained_media(cur) {
        return Ok((ct, HashMap::new()));
    }
    cur.pos = saved;
    // Content-general-form = Value-length Media-type
    let len = decode_value_length(cur)?;
    let end = cur.pos + len;
    if end > cur.data.len() {
        return Err(());
    }
    let media = if let Ok(id) = decode_integer_value(cur) {
        well_known_content_type(id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("application/octet-stream;id={id}"))
    } else {
        decode_text_string(cur)?
    };
    let mut params = HashMap::new();
    // Best-effort parameter scan inside remaining general-form bytes.
    while cur.pos < end {
        let pstart = cur.pos;
        if let Ok(name_id) = decode_short_integer(cur) {
            if name_id == 0x05 || name_id == 0x17 {
                // Name / Filename
                if let Ok(val) = decode_text_string(cur).or_else(|_| decode_encoded_string_value(cur))
                {
                    params.insert("Name".into(), val);
                    continue;
                }
            }
            if name_id == 0x0a || name_id == 0x19 {
                if let Ok(val) = decode_text_string(cur).or_else(|_| decode_encoded_string_value(cur))
                {
                    params.insert("Start".into(), val);
                    continue;
                }
            }
        }
        cur.pos = pstart + 1;
        if cur.pos <= pstart {
            break;
        }
    }
    cur.pos = end;
    Ok((media, params))
}

fn skip_unknown_mms_value(cur: &mut Cursor<'_>) -> Result<(), ()> {
    // Best-effort: value-length blob, short-integer, or text-string.
    let saved = cur.pos;
    if let Ok(len) = decode_value_length(cur) {
        let _ = cur.take(len)?;
        return Ok(());
    }
    cur.pos = saved;
    if decode_short_integer(cur).is_ok() {
        return Ok(());
    }
    cur.pos = saved;
    let _ = decode_text_string(cur)?;
    Ok(())
}

fn decode_mms_header_field(cur: &mut Cursor<'_>, msg: &mut StructuredMms) -> Result<bool, ()> {
    let byte = cur.peek().ok_or(())?;
    if byte & 0x80 == 0 {
        // Application-header (text name) — skip name + value.
        let _name = decode_text_string(cur)?;
        let _ = skip_unknown_mms_value(cur)?;
        return Ok(false);
    }
    let field = decode_short_integer(cur)?;
    match field {
        MMS_FROM => {
            msg.from = Some(decode_from_value(cur)?);
            Ok(false)
        }
        MMS_TO => {
            msg.to.push(decode_encoded_string_value(cur)?);
            Ok(false)
        }
        MMS_CC => {
            msg.cc.push(decode_encoded_string_value(cur)?);
            Ok(false)
        }
        MMS_MESSAGE_TYPE => {
            msg.message_type = Some(decode_message_type_value(cur)?);
            Ok(false)
        }
        MMS_DATE => {
            msg.date_unix = Some(decode_date_value(cur)?);
            Ok(false)
        }
        MMS_CONTENT_TYPE => {
            let (ct, _params) = decode_content_type_value(cur)?;
            msg.content_type = Some(ct);
            Ok(true) // Content-Type terminates the header section
        }
        MMS_CONTENT_LOCATION => {
            let _ = decode_encoded_string_value(cur)
                .or_else(|_| decode_text_string(cur))
                .or_else(|_| {
                    skip_unknown_mms_value(cur)?;
                    Ok(String::new())
                })?;
            Ok(false)
        }
        _ => {
            skip_unknown_mms_value(cur)?;
            Ok(false)
        }
    }
}

fn decode_multipart_body(cur: &mut Cursor<'_>) -> Result<Vec<MmsPart>, ()> {
    let n = decode_uint_var(cur)? as usize;
    if n > 256 {
        return Err(());
    }
    let mut parts = Vec::with_capacity(n);
    for _ in 0..n {
        let headers_len = decode_uint_var(cur)? as usize;
        let data_len = decode_uint_var(cur)? as usize;
        if headers_len + data_len > cur.remaining() {
            return Err(());
        }
        let header_bytes = cur.take(headers_len)?;
        let mut hcur = Cursor::new(header_bytes);
        let (ctype, params) = decode_content_type_value(&mut hcur).unwrap_or_else(|_| {
            ("application/octet-stream".into(), HashMap::new())
        });
        let mut content_location = params.get("Name").cloned();
        while hcur.remaining() > 0 {
            let before = hcur.pos;
            if let Ok(field) = decode_short_integer(&mut hcur) {
                if field == MMS_CONTENT_LOCATION || field == 0x0e {
                    // Content-Location / Content-ID-ish
                    if let Ok(v) = decode_encoded_string_value(&mut hcur)
                        .or_else(|_| decode_text_string(&mut hcur))
                    {
                        content_location = Some(v);
                        continue;
                    }
                } else {
                    let _ = skip_unknown_mms_value(&mut hcur);
                }
            } else if decode_text_string(&mut hcur).is_ok() {
                let _ = skip_unknown_mms_value(&mut hcur);
            }
            if hcur.pos == before {
                hcur.pos += 1;
            }
        }
        let data = cur.take(data_len)?.to_vec();
        parts.push(MmsPart {
            content_type: ctype,
            content_location,
            data,
        });
    }
    Ok(parts)
}

/// Attempt a full WAP-209 header + multipart decode from the start of `data`.
pub fn decode_mms(data: &[u8]) -> Option<StructuredMms> {
    if data.len() < 4 {
        return None;
    }
    let mut cur = Cursor::new(data);
    let mut msg = StructuredMms::default();
    let mut saw_content_type = false;
    for _ in 0..64 {
        match decode_mms_header_field(&mut cur, &mut msg) {
            Ok(true) => {
                saw_content_type = true;
                break;
            }
            Ok(false) => {}
            Err(()) => return None,
        }
    }
    if !saw_content_type && msg.from.is_none() && msg.to.is_empty() {
        return None;
    }
    if let Some(ct) = &msg.content_type {
        if ct.contains("multipart") {
            if let Ok(parts) = decode_multipart_body(&mut cur) {
                msg.parts = parts;
            }
        }
    }
    if msg.is_useful() {
        Some(msg)
    } else {
        None
    }
}

fn is_printable_name_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-')
}

fn looks_like_part_name(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    if !name.contains('.') {
        return false;
    }
    name.bytes().all(is_printable_name_byte)
}

fn try_parse_cloc_name_at(data: &[u8], at: usize) -> Option<(String, usize)> {
    // at points at 0x8e; returns (name, index of byte after NUL).
    if at >= data.len() || data[at] != 0x8e || at + 1 >= data.len() {
        return None;
    }
    let next = data[at + 1];
    if next & 0x80 != 0 || !is_printable_name_byte(next) {
        return None;
    }
    let name_start = at + 1;
    let mut name_end = name_start;
    while name_end < data.len() && data[name_end] != 0 {
        if !is_printable_name_byte(data[name_end]) {
            return None;
        }
        name_end += 1;
    }
    if name_end >= data.len() || data[name_end] != 0 {
        return None;
    }
    let name = String::from_utf8_lossy(&data[name_start..name_end]).into_owned();
    if !looks_like_part_name(&name) {
        return None;
    }
    Some((name, name_end + 1))
}

fn find_next_cloc_name(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 2 < data.len() {
        if data[i] == 0x8e && try_parse_cloc_name_at(data, i).is_some() {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_text_part_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("text") && lower.ends_with(".txt")
}

/// Scan Content-Location (`0x8e`) + NUL-terminated name + payload.
///
/// Text parts end at the next short-integer header byte. Media parts end at the
/// next Content-Location name (or EOF) so JPEG/etc. high bytes are kept intact.
pub fn scan_named_parts(data: &[u8]) -> Vec<NamedPart> {
    let mut parts = Vec::new();
    let mut i = 0;
    while i + 2 < data.len() {
        let Some((name, payload_start)) = try_parse_cloc_name_at(data, i) else {
            i += 1;
            continue;
        };
        let payload_end = if is_text_part_name(&name) {
            let mut end = payload_start;
            while end < data.len() && data[end] & 0x80 == 0 {
                end += 1;
            }
            end
        } else {
            find_next_cloc_name(data, payload_start).unwrap_or(data.len())
        };
        let payload = data[payload_start..payload_end].to_vec();
        parts.push(NamedPart { name, data: payload });
        i = payload_end.max(i + 1);
    }
    parts
}

pub fn content_type_from_filename(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "txt" => "text/plain".into(),
        "html" | "htm" => "text/html".into(),
        "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        "gif" => "image/gif".into(),
        "amr" => "audio/amr".into(),
        "mp3" => "audio/mpeg".into(),
        "wav" => "audio/wav".into(),
        "3gp" => "video/3gpp".into(),
        "mp4" => "video/mp4".into(),
        "smil" => "application/smil".into(),
        _ => "application/octet-stream".into(),
    }
}

fn merge_named_parts(msg: &mut StructuredMms, named: Vec<NamedPart>) {
    if named.is_empty() {
        return;
    }
    for np in &named {
        let already = msg.parts.iter().any(|p| {
            p.content_location.as_deref() == Some(np.name.as_str())
                || (p.data == np.data && !np.data.is_empty())
        });
        if already {
            continue;
        }
        msg.parts.push(MmsPart {
            content_type: content_type_from_filename(&np.name),
            content_location: Some(np.name.clone()),
            data: np.data.clone(),
        });
    }
    msg.named_parts = named;
}

/// Scan for embedded From/To/Cc/Date short-integer headers (GO SMS Pro fragments).
pub fn scan_mms_addresses(data: &[u8]) -> StructuredMms {
    let mut msg = StructuredMms::default();
    let mut i = 0;
    while i + 2 < data.len() {
        let byte = data[i];
        if byte & 0x80 == 0 {
            i += 1;
            continue;
        }
        let field = byte & 0x7f;
        let mut cur = Cursor {
            data,
            pos: i + 1,
        };
        match field {
            MMS_FROM => {
                if let Ok(addr) = decode_from_value(&mut cur) {
                    if !addr.is_empty() {
                        msg.from = Some(addr);
                        i = cur.pos;
                        continue;
                    }
                }
            }
            MMS_TO => {
                if let Ok(addr) = decode_encoded_string_value(&mut cur) {
                    if !addr.is_empty() {
                        msg.to.push(addr);
                        i = cur.pos;
                        continue;
                    }
                }
            }
            MMS_CC => {
                if let Ok(addr) = decode_encoded_string_value(&mut cur) {
                    if !addr.is_empty() {
                        msg.cc.push(addr);
                        i = cur.pos;
                        continue;
                    }
                }
            }
            MMS_DATE => {
                if let Ok(d) = decode_date_value(&mut cur) {
                    if d > 0 && msg.date_unix.is_none() {
                        msg.date_unix = Some(d);
                        i = cur.pos;
                        continue;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    msg
}

/// Prefer a full decode; always harvest named Content-Location parts and fragment headers.
pub fn decode_mms_best_effort(data: &[u8]) -> StructuredMms {
    let named = scan_named_parts(data);
    let scanned = scan_mms_addresses(data);
    let mut msg = if let Some(mut full) = decode_mms(data) {
        if full.date_unix.is_none() {
            full.date_unix = scanned.date_unix;
        }
        if full.from.is_none() {
            full.from = scanned.from;
        }
        if full.to.is_empty() {
            full.to = scanned.to;
        }
        if full.cc.is_empty() {
            full.cc = scanned.cc;
        }
        full
    } else {
        scanned
    };
    merge_named_parts(&mut msg, named);
    msg
}

pub fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    let ct = content_type.to_ascii_lowercase();
    let base = ct.split(';').next().unwrap_or(&ct).trim();
    match base {
        "image/jpeg" | "image/jpg" => Some(".jpg"),
        "image/png" => Some(".png"),
        "image/gif" => Some(".gif"),
        "image/tiff" => Some(".tiff"),
        "image/vnd.wap.wbmp" => Some(".wbmp"),
        "audio/amr" | "audio/3gpp" => Some(".amr"),
        "audio/mpeg" | "audio/mp3" => Some(".mp3"),
        "audio/wav" | "audio/x-wav" => Some(".wav"),
        "video/3gpp" => Some(".3gp"),
        "video/mp4" => Some(".mp4"),
        "text/plain" | "text/*" => Some(".txt"),
        "application/smil" | "application/vnd.wap.multipart.related" => None,
        _ if base.starts_with("image/") => Some(".bin"),
        _ if base.starts_with("audio/") => Some(".bin"),
        _ if base.starts_with("video/") => Some(".bin"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn uint_var_single_and_multi() {
        let mut c = Cursor::new(&[0x05]);
        assert_eq!(decode_uint_var(&mut c).unwrap(), 5);
        // 0x81 0x02 => (1<<7)|2 = 130
        let mut c = Cursor::new(&[0x81, 0x02]);
        assert_eq!(decode_uint_var(&mut c).unwrap(), 130);
    }

    #[test]
    fn value_length_short_and_quoted() {
        let mut c = Cursor::new(&[0x1a]);
        assert_eq!(decode_value_length(&mut c).unwrap(), 26);
        let mut c = Cursor::new(&[0x1f, 0x20]);
        assert_eq!(decode_value_length(&mut c).unwrap(), 32);
    }

    #[test]
    fn scan_from_to_on_recv_fixture_shape() {
        // From + To fragment matching I_1609459200_recv.pdu: Value-lengths overshoot
        // into the next short-integer header (no NULs after PLMN).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        bytes.extend_from_slice(&[0x8e]); // next header byte overlapped by To length
        let msg = scan_mms_addresses(&bytes);
        assert_eq!(msg.from.as_deref(), Some("+4075551234/TYPE=PLMN"));
        assert_eq!(msg.to, vec!["+15555550100/TYPE=PLMN".to_string()]);
    }

    #[test]
    fn scan_real_recv_fixture() {
        let data = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/pdu/I_1609459200_recv.pdu"),
        )
        .unwrap();
        let msg = decode_mms_best_effort(&data);
        assert_eq!(msg.from.as_deref(), Some("+4075551234/TYPE=PLMN"));
        assert!(msg.to.iter().any(|t| t.contains("15555550100")));
        assert!(
            msg.named_parts
                .iter()
                .any(|p| p.name == "text.txt" && p.data.starts_with(b"Hello one to one"))
        );
    }

    #[test]
    fn scan_date_header_fragment() {
        // Date = 0x85, long-integer length 4, value 0x5fee6600 = 1609459200
        let mut bytes = vec![0x85, 0x04, 0x5f, 0xee, 0x66, 0x00];
        bytes.extend_from_slice(&[0x8e]);
        bytes.extend_from_slice(b"text.txt\0hi");
        let msg = decode_mms_best_effort(&bytes);
        assert_eq!(msg.date_unix, Some(1609459200));
        assert_eq!(msg.named_parts[0].name, "text.txt");
        assert_eq!(msg.named_parts[0].data, b"hi");
    }
}
