/*!
 Export format selection.
*/

use std::fmt::Display;

/// Export file format.
#[derive(PartialEq, Eq, Debug)]
pub enum ExportType {
    /// HTML export.
    Html,
    /// Plain text export.
    Txt,
    /// Per-conversation CSV export (HTML message surface fields, DB-backed).
    Csv,
}

impl ExportType {
    /// Parse an export format from CLI input.
    pub fn from_cli(format: &str) -> Option<Self> {
        match format.to_lowercase().as_str() {
            "txt" => Some(Self::Txt),
            "html" => Some(Self::Html),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    /// Return the file extension for this export format.
    pub fn extension(&self) -> &str {
        match self {
            ExportType::Html => ".html",
            ExportType::Txt => ".txt",
            ExportType::Csv => ".csv",
        }
    }
}

impl Display for ExportType {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportType::Txt => write!(fmt, "txt"),
            ExportType::Html => write!(fmt, "html"),
            ExportType::Csv => write!(fmt, "csv"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::export_type::ExportType;

    #[test]
    fn can_parse_html_any_case() {
        assert!(matches!(
            ExportType::from_cli("html"),
            Some(ExportType::Html)
        ));
        assert!(matches!(
            ExportType::from_cli("HTML"),
            Some(ExportType::Html)
        ));
        assert!(matches!(
            ExportType::from_cli("HtMl"),
            Some(ExportType::Html)
        ));
    }

    #[test]
    fn can_parse_txt_any_case() {
        assert!(matches!(ExportType::from_cli("txt"), Some(ExportType::Txt)));
        assert!(matches!(ExportType::from_cli("TXT"), Some(ExportType::Txt)));
        assert!(matches!(ExportType::from_cli("tXt"), Some(ExportType::Txt)));
    }

    #[test]
    fn can_parse_csv_any_case() {
        assert!(matches!(ExportType::from_cli("csv"), Some(ExportType::Csv)));
        assert!(matches!(ExportType::from_cli("CSV"), Some(ExportType::Csv)));
        assert!(matches!(ExportType::from_cli("CsV"), Some(ExportType::Csv)));
    }

    #[test]
    fn cant_parse_invalid() {
        assert!(ExportType::from_cli("pdf").is_none());
        assert!(ExportType::from_cli("json").is_none());
        assert!(ExportType::from_cli("").is_none());
    }
}
