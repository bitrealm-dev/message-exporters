//! Shared iMazing Contacts CSV column layout.

/// Phone / fax columns used by iMazing Contacts exports.
pub(crate) const IMAZING_PHONE_COLUMNS: &[&str] = &[
    "mobile phone",
    "home phone",
    "work phone",
    "other phone",
    "home fax",
    "work fax",
    "other fax",
];

/// Normalize a Contacts CSV header the same way book/validate loaders do.
pub(crate) fn normalize_imazing_header(h: &str) -> String {
    h.trim().to_ascii_lowercase()
}

/// Column indexes for an iMazing Contacts CSV header row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImazingContactsColumns {
    pub first: Option<usize>,
    pub middle: Option<usize>,
    pub last: Option<usize>,
    pub notes: Option<usize>,
    pub phones: Vec<usize>,
}

impl ImazingContactsColumns {
    /// Resolve column indexes from raw header names.
    pub(crate) fn from_headers<I, S>(headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let headers: Vec<String> = headers
            .into_iter()
            .map(|h| normalize_imazing_header(h.as_ref()))
            .collect();
        Self {
            first: headers.iter().position(|h| h == "first name"),
            middle: headers.iter().position(|h| h == "middle name"),
            last: headers.iter().position(|h| h == "last name"),
            notes: headers.iter().position(|h| h == "notes"),
            phones: IMAZING_PHONE_COLUMNS
                .iter()
                .filter_map(|name| headers.iter().position(|h| h == *name))
                .collect(),
        }
    }

    /// True when the header looks like an iMazing Contacts export.
    pub(crate) fn looks_like_imazing(&self) -> bool {
        self.first.is_some() || !self.phones.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_name_and_phone_columns() {
        let cols = ImazingContactsColumns::from_headers([
            "First Name",
            "Middle Name",
            "Last Name",
            "Mobile Phone",
            "Notes",
        ]);
        assert_eq!(cols.first, Some(0));
        assert_eq!(cols.middle, Some(1));
        assert_eq!(cols.last, Some(2));
        assert_eq!(cols.phones, vec![3]);
        assert_eq!(cols.notes, Some(4));
        assert!(cols.looks_like_imazing());
    }
}
