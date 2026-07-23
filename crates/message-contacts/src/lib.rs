//! Contact books for backup→CSV exporters: validated CSV/VCF → name↔phone indexes.
//!
//! Name resolution belongs here (not in vault csv-ingest). CSV is the human checkpoint;
//! exporters should write correct handles and display names before that stage.
//!
//! Accepted inputs match contacts-validate: VCF, or iMazing Contacts CSV
//! (First Name, Last Name, phone columns). Legacy vault CSV is not supported.

mod book;
mod mapping;
mod name;
mod validate;
mod vcf;

pub use book::{resolve_contacts_cli, ContactsBook};
pub use mapping::NameMapping;
pub use name::{collapse_inner_whitespace, normalize_name_key};
pub use validate::{
    detect_contacts_format, ensure_contacts_input, probe_contacts_input, validate_contacts_file,
    ContactsFormat, ContactsInputError, ValidateMode, ValidateReport, UNRECOGNIZED_CONTACTS_FORMAT,
};
