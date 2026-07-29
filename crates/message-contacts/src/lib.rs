//! Contact books for backup→CSV exporters: validated CSV/VCF → name↔phone indexes.
//!
//! Name resolution belongs here (not in vault csv-ingest). CSV is the human checkpoint;
//! exporters should write correct handles and display names before that stage.
//!
//! Accepted inputs match contacts-validate: VCF, or iMazing Contacts CSV
//! (First Name, Last Name, phone columns).

mod book;
mod imazing_csv;
mod mapping;
mod name;
mod validate;
mod vcf;

pub use book::{ContactsBook, resolve_contacts_cli};
pub use mapping::NameMapping;
pub use validate::{
    ContactsInputError, ValidateMode, ValidateReport, probe_contacts_input, validate_contacts_file,
};
