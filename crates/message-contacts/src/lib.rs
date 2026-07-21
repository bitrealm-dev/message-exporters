//! Contact books for backup→CSV exporters: vault-shaped CSV or VCF → name↔phone indexes.
//!
//! Name resolution belongs here (not in vault csv-ingest). CSV is the human checkpoint;
//! exporters should write correct handles and display names before that stage.

mod book;
mod mapping;
mod name;
mod validate;
mod vcf;

pub use book::{resolve_contacts_cli, ContactsBook};
pub use mapping::NameMapping;
pub use name::{collapse_inner_whitespace, normalize_name_key};
pub use validate::{validate_contacts_file, ValidateMode, ValidateReport};
