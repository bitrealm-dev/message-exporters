//! Name normalization for contact book lookups.

/// Collapse runs of whitespace to a single space and trim.
pub fn collapse_inner_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalize a contact / export name for map lookup.
///
/// Lowercases, treats `_` as space, strips trailing `__SUFFIX` markers.
pub fn normalize_name_key(name: &str) -> String {
    let mut s = name.trim().to_string();
    if let Some(idx) = s.find("__") {
        s.truncate(idx);
    }
    s = s.replace('_', " ");
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// True when a display name is empty or a placeholder like `Unknown`.
pub fn is_blank_or_unknown_name(name: &str) -> bool {
    let key = normalize_name_key(name);
    key.is_empty() || key == "unknown" || key == "me"
}
