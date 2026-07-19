use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

/// Alphabetically sorted by display name.
pub const EXPORTERS: [Exporter; 6] = [
    Exporter::GoSmsPro,
    Exporter::Imazing,
    Exporter::Imessage,
    Exporter::OpenExtract,
    Exporter::SmsBackupRestore,
    Exporter::SmsBackupPlus,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Exporter {
    #[default]
    GoSmsPro,
    Imazing,
    Imessage,
    OpenExtract,
    SmsBackupRestore,
    SmsBackupPlus,
}

impl Exporter {
    pub fn binary(self) -> &'static str {
        match self {
            Self::GoSmsPro => "go-sms-pro-out",
            Self::SmsBackupRestore => "sms-backup-restore-out",
            Self::SmsBackupPlus => "sms-backup-plus-out",
            Self::OpenExtract => "openextract-out",
            Self::Imazing => "imazing-out",
            Self::Imessage => "imessage-exporter",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::GoSmsPro => "GO SMS Pro",
            Self::SmsBackupRestore => "SMS Backup & Restore",
            Self::SmsBackupPlus => "SMS Backup+",
            Self::OpenExtract => "OpenExtract",
            Self::Imazing => "iMazing",
            Self::Imessage => "iPhone backup",
        }
    }

    /// Form title / hyperlink text (may be longer than the dropdown label).
    pub fn link_label(self) -> &'static str {
        match self {
            Self::Imessage => "iPhone backup - imessage-exporter",
            other => other.display_name(),
        }
    }

    pub fn product_url(self) -> &'static str {
        match self {
            Self::GoSmsPro => "https://play.google.com/store/apps/details?id=com.jb.gosms",
            Self::SmsBackupRestore => "https://www.synctech.com.au/sms-backup-restore/",
            Self::SmsBackupPlus => "https://github.com/jberkel/sms-backup-plus",
            Self::OpenExtract => "https://www.openextract.app/",
            Self::Imazing => "https://imazing.com/",
            Self::Imessage => "https://github.com/ReagentX/imessage-exporter",
        }
    }

    pub fn output_subdir(self) -> &'static str {
        match self {
            Self::GoSmsPro => "go-sms-pro",
            Self::SmsBackupRestore => "sms-backup-restore",
            Self::SmsBackupPlus => "sms-backup-plus",
            Self::OpenExtract => "openextract",
            Self::Imazing => "imazing",
            Self::Imessage => "iphone-backup",
        }
    }
}

/// Documents/message-exporters/<source> (or home fallback).
pub fn default_output_dir(exporter: Exporter) -> String {
    let root = dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    root.join("message-exporters")
        .join(exporter.output_subdir())
        .display()
        .to_string()
}

impl fmt::Display for Exporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContactsKind {
    #[default]
    None,
    Csv,
    Vcf,
}

impl fmt::Display for ContactsKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "No contacts",
            Self::Csv => "Contacts CSV",
            Self::Vcf => "Contacts VCF",
        })
    }
}

pub const CONTACT_KINDS: [ContactsKind; 3] =
    [ContactsKind::None, ContactsKind::Csv, ContactsKind::Vcf];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopyMethod {
    #[default]
    Clone,
    Basic,
    Full,
    Disabled,
}

impl fmt::Display for CopyMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Clone => "Clone (recommended)",
            Self::Basic => "Basic conversion",
            Self::Full => "Full conversion",
            Self::Disabled => "Do not copy",
        })
    }
}

impl CopyMethod {
    fn cli(self) -> &'static str {
        match self {
            Self::Clone => "clone",
            Self::Basic => "basic",
            Self::Full => "full",
            Self::Disabled => "disabled",
        }
    }
}

pub const COPY_METHODS: [CopyMethod; 4] = [
    CopyMethod::Clone,
    CopyMethod::Basic,
    CopyMethod::Full,
    CopyMethod::Disabled,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApplePlatform {
    #[default]
    Auto,
    MacOs,
    Ios,
}

impl fmt::Display for ApplePlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "Auto-detect",
            Self::MacOs => "macOS",
            Self::Ios => "iOS backup",
        })
    }
}

pub const APPLE_PLATFORMS: [ApplePlatform; 3] = [
    ApplePlatform::Auto,
    ApplePlatform::MacOs,
    ApplePlatform::Ios,
];

#[derive(Debug, Clone)]
pub struct Form {
    pub input: String,
    pub output: String,
    pub contacts: String,
    pub contacts_kind: ContactsKind,
    pub owner_phones: String,
    pub owner_emails: String,
    pub name_mapping: String,
    pub timezone: String,
    pub anonymize: bool,
    pub anonymize_seed: String,
    pub advanced: bool,
    pub db_path: String,
    pub attachment_root: String,
    pub start_date: String,
    pub end_date: String,
    pub conversation_filter: String,
    pub apple_contacts: String,
    pub backup_password: String,
    pub copy_method: CopyMethod,
    pub apple_platform: ApplePlatform,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            input: String::new(),
            output: default_output_dir(Exporter::default()),
            contacts: String::new(),
            contacts_kind: ContactsKind::default(),
            owner_phones: String::new(),
            owner_emails: String::new(),
            name_mapping: String::new(),
            timezone: String::new(),
            anonymize: false,
            anonymize_seed: String::new(),
            advanced: false,
            db_path: String::new(),
            attachment_root: String::new(),
            start_date: String::new(),
            end_date: String::new(),
            conversation_filter: String::new(),
            apple_contacts: String::new(),
            backup_password: String::new(),
            copy_method: CopyMethod::default(),
            apple_platform: ApplePlatform::default(),
        }
    }
}

impl Form {
    pub fn build_args(&self, exporter: Exporter) -> Result<Vec<OsString>, Vec<String>> {
        let mut errors = Vec::new();
        let mut args = Vec::<OsString>::new();

        match exporter {
            Exporter::Imessage => self.build_imessage(&mut args, &mut errors),
            _ => {
                if exporter == Exporter::SmsBackupPlus {
                    args.push("convert".into());
                }
                required_single_path(&self.input, "Input", &mut errors);
                required_text(&self.output, "Output", &mut errors);

                push_pair(&mut args, "--input", &self.input);
                push_pair(&mut args, "--output", &self.output);

                if matches!(
                    exporter,
                    Exporter::GoSmsPro | Exporter::SmsBackupRestore | Exporter::SmsBackupPlus
                ) {
                    let phones = values(&self.owner_phones);
                    if phones.is_empty() {
                        errors.push("At least one phone number is required.".into());
                    }
                    for phone in phones {
                        push_pair(&mut args, "--owner-phone", phone);
                    }
                }
                if exporter == Exporter::SmsBackupPlus {
                    let emails = values(&self.owner_emails);
                    if emails.is_empty() {
                        errors.push("At least one email address is required.".into());
                    }
                    for email in emails {
                        push_pair(&mut args, "--owner-email", email);
                    }
                    push_optional_pair(&mut args, "--name-mapping", &self.name_mapping);
                    args.push("--verbose".into());
                }

                match exporter {
                    Exporter::Imazing => {
                        push_optional_pair(&mut args, "--contacts", &self.contacts);
                        push_optional_pair(&mut args, "--timezone", &self.timezone);
                    }
                    _ => match self.contacts_kind {
                        ContactsKind::None => {}
                        ContactsKind::Csv => {
                            if self.contacts.trim().is_empty() {
                                errors.push("Choose a contacts CSV or select No contacts.".into());
                            } else {
                                push_pair(&mut args, "--contacts", &self.contacts);
                            }
                        }
                        ContactsKind::Vcf => {
                            if self.contacts.trim().is_empty() {
                                errors.push("Choose a contacts VCF or select No contacts.".into());
                            } else {
                                push_pair(&mut args, "--vcf", &self.contacts);
                            }
                        }
                    },
                }

                if exporter != Exporter::Imazing {
                    if self.anonymize {
                        args.push("--anonymize".into());
                    }
                    push_seed(&mut args, &self.anonymize_seed, &mut errors);
                }
            }
        }

        if errors.is_empty() {
            Ok(args)
        } else {
            Err(errors)
        }
    }

    fn build_imessage(&self, args: &mut Vec<OsString>, errors: &mut Vec<String>) {
        required_text(&self.output, "Output directory", errors);
        args.extend(["--format".into(), "csv".into()]);
        args.extend(["--copy-method".into(), self.copy_method.cli().into()]);
        push_pair(args, "--export-path", &self.output);
        push_optional_pair(args, "--db-path", &self.db_path);
        push_optional_pair(args, "--attachment-root", &self.attachment_root);
        push_optional_pair(args, "--start-date", &self.start_date);
        push_optional_pair(args, "--end-date", &self.end_date);
        push_optional_pair(args, "--conversation-filter", &self.conversation_filter);
        push_optional_pair(args, "--contacts-path", &self.apple_contacts);
        push_optional_pair(args, "--cleartext-password", &self.backup_password);
        match self.apple_platform {
            ApplePlatform::Auto => {}
            ApplePlatform::MacOs => args.extend(["--platform".into(), "macOS".into()]),
            ApplePlatform::Ios => args.extend(["--platform".into(), "iOS".into()]),
        }
        args.push("--use-caller-id".into());
        if self.anonymize {
            args.push("--anonymize".into());
        }
        push_seed(args, &self.anonymize_seed, errors);
    }
}

fn required_single_path(value: &str, label: &str, errors: &mut Vec<String>) {
    let paths = lines(value);
    if paths.is_empty() {
        errors.push(format!("{label} is required."));
        return;
    }
    if paths.len() > 1 {
        errors.push(format!("{label} must be a single file or folder."));
        return;
    }
    let path = paths[0];
    if !Path::new(path).exists() {
        errors.push(format!("{label} path does not exist: {path}"));
    }
}

fn required_text(value: &str, label: &str, errors: &mut Vec<String>) {
    if value.trim().is_empty() {
        errors.push(format!("{label} is required."));
    }
}

fn push_seed(args: &mut Vec<OsString>, seed: &str, errors: &mut Vec<String>) {
    let seed = seed.trim();
    if seed.is_empty() {
        return;
    }
    if seed.len() != 64 || !seed.chars().all(|c| c.is_ascii_hexdigit()) {
        errors.push("Anonymize seed must be exactly 64 hexadecimal characters.".into());
    } else {
        push_pair(args, "--anonymize-seed", seed);
    }
}

fn push_optional_pair(args: &mut Vec<OsString>, flag: &str, value: &str) {
    if !value.trim().is_empty() {
        push_pair(args, flag, value);
    }
}

fn push_pair(args: &mut Vec<OsString>, flag: &str, value: &str) {
    args.push(flag.into());
    args.push(value.trim().into());
}

fn lines(value: &str) -> Vec<&str> {
    value
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect()
}

fn values(value: &str) -> Vec<&str> {
    value
        .split(['\n', ',', ';'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imazing_omits_unsupported_anonymize() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            anonymize: true,
            ..Form::default()
        };
        let args = form.build_args(Exporter::Imazing).unwrap();
        assert!(!args.iter().any(|arg| arg == "--anonymize"));
    }

    #[test]
    fn seed_must_be_64_hex() {
        let form = Form {
            input: std::env::current_dir().unwrap().display().to_string(),
            output: "out".into(),
            anonymize_seed: "bad".into(),
            ..Form::default()
        };
        assert!(form.build_args(Exporter::OpenExtract).is_err());
    }

    #[test]
    fn plus_prefixes_convert_always_verbose_and_single_input() {
        let cwd = std::env::current_dir().unwrap().display().to_string();
        let form = Form {
            input: cwd,
            output: "out".into(),
            owner_phones: "+15555550100\n+15555550101".into(),
            owner_emails: "me@example.com".into(),
            ..Form::default()
        };
        let args = form.build_args(Exporter::SmsBackupPlus).unwrap();
        assert_eq!(args.first().unwrap(), "convert");
        assert_eq!(args.iter().filter(|arg| *arg == "--owner-phone").count(), 2);
        assert_eq!(args.iter().filter(|arg| *arg == "--input").count(), 1);
        assert!(args.iter().any(|arg| arg == "--verbose"));
        assert!(!args.iter().any(|arg| arg == "--no-summary"));
    }

    #[test]
    fn plus_rejects_multiple_inputs() {
        let cwd = std::env::current_dir().unwrap().display().to_string();
        let form = Form {
            input: format!("{cwd}\n{cwd}"),
            output: "out".into(),
            owner_phones: "+15555550100".into(),
            owner_emails: "me@example.com".into(),
            ..Form::default()
        };
        let err = form.build_args(Exporter::SmsBackupPlus).unwrap_err();
        assert!(err.iter().any(|e| e.contains("single file or folder")));
    }

    #[test]
    fn exporters_are_alphabetical_by_display_name() {
        let names: Vec<_> = EXPORTERS.iter().map(|e| e.display_name()).collect();
        let mut sorted = names.clone();
        sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        assert_eq!(names, sorted);
    }

    #[test]
    fn imessage_requires_output_and_always_uses_caller_id() {
        let form = Form {
            output: String::new(),
            ..Form::default()
        };
        assert!(form.build_args(Exporter::Imessage).is_err());

        let form = Form {
            output: default_output_dir(Exporter::Imessage),
            ..Form::default()
        };
        let args = form.build_args(Exporter::Imessage).unwrap();
        assert!(args.iter().any(|arg| arg == "--export-path"));
        assert!(args.iter().any(|arg| arg == "--use-caller-id"));
        assert!(!args.iter().any(|arg| arg == "--custom-name"));
        assert!(!args.iter().any(|arg| arg == "--ignore-disk-warning"));
        assert!(!args.iter().any(|arg| arg == "--diagnostics"));
    }

    #[test]
    fn default_output_is_under_documents_or_home() {
        let path = default_output_dir(Exporter::OpenExtract);
        assert!(path.contains("message-exporters"));
        assert!(path.contains("openextract"));
    }
}
