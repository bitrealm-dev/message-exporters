//! Small shared helpers for IR readers / projectors.

/// Stem packaging suffix used for WhatsApp exports (`__whatsapp`).
pub(crate) fn packaging_suffix_from_stem(stem: &str) -> Option<String> {
    if stem.ends_with("__whatsapp") {
        Some("__whatsapp".into())
    } else {
        None
    }
}
