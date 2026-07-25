//! Load/save GUI export options as INI (`export.ini`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use ini::Ini;
use message_media::MaxResolution;

use crate::exporters::{
    contacts_kind_from_path, ApplePlatform, AttachmentMedia, Exporter, Form, EXPORTERS,
};

const COMMON: &str = "common";
pub const EXPORT_INI_NAME: &str = "export.ini";

/// Per-exporter path / type-specific fields kept when switching backup types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExporterSection {
    pub input: String,
    pub output: String,
    pub owner_emails: String,
    pub name_mapping: String,
    pub timezone: String,
    pub apple_platform: ApplePlatform,
    pub apple_contacts: String,
    pub attachment_root: String,
    pub conversation_filter: String,
}

/// In-memory export.ini plus the path used for load/save.
#[derive(Debug, Clone)]
pub struct ExportIniState {
    pub path: PathBuf,
    pub exporter: Exporter,
    sections: [ExporterSection; 6],
}

impl ExportIniState {
    fn section_mut(&mut self, exporter: Exporter) -> &mut ExporterSection {
        &mut self.sections[exporter_index(exporter)]
    }

    fn section(&self, exporter: Exporter) -> &ExporterSection {
        &self.sections[exporter_index(exporter)]
    }

    /// Resolve path, load if present, otherwise empty defaults at the preferred path.
    pub fn load_or_default() -> (Self, Form) {
        let path = resolve_export_ini_path();
        match Self::load(&path) {
            Ok((state, form)) => (state, form),
            Err(_) => {
                let state = Self {
                    path,
                    exporter: Exporter::default(),
                    sections: Default::default(),
                };
                let mut form = Form::default();
                state.apply_section_to_form(&mut form);
                (state, form)
            }
        }
    }

    pub fn load(path: &Path) -> Result<(Self, Form), String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        let ini = Ini::load_from_str(&text)
            .map_err(|e| format!("Could not parse {}: {e}", path.display()))?;

        let mut form = Form::default();
        apply_common(&ini, &mut form);

        let exporter = ini
            .get_from(Some(COMMON), "exporter")
            .and_then(Exporter::from_ini_key)
            .unwrap_or_default();

        let mut sections: [ExporterSection; 6] = Default::default();
        for (i, exp) in EXPORTERS.iter().copied().enumerate() {
            sections[i] = read_section(&ini, exp);
        }

        let state = Self {
            path: path.to_path_buf(),
            exporter,
            sections,
        };
        state.apply_section_to_form(&mut form);
        Ok((state, form))
    }

    /// Copy the active exporter's section fields into `form`.
    pub fn apply_section_to_form(&self, form: &mut Form) {
        let section = self.section(self.exporter);
        match self.exporter {
            Exporter::Imessage => {
                form.db_path = section.input.clone();
                form.input.clear();
            }
            _ => {
                form.input = section.input.clone();
                form.db_path.clear();
            }
        }
        form.output = section.output.clone();
        form.owner_emails = section.owner_emails.clone();
        form.name_mapping = section.name_mapping.clone();
        form.timezone = section.timezone.clone();
        form.apple_platform = section.apple_platform;
        form.apple_contacts = section.apple_contacts.clone();
        form.attachment_root = section.attachment_root.clone();
        form.conversation_filter = section.conversation_filter.clone();
        form.contacts_kind = contacts_kind_from_path(&form.contacts);
    }

    /// Store type-specific fields from `form` into the active exporter section.
    pub fn capture_form_section(&mut self, form: &Form) {
        let exporter = self.exporter;
        let section = self.section_mut(exporter);
        section.input = match exporter {
            Exporter::Imessage => form.db_path.clone(),
            _ => form.input.clone(),
        };
        section.output = form.output.clone();
        section.owner_emails = form.owner_emails.clone();
        section.name_mapping = form.name_mapping.clone();
        section.timezone = form.timezone.clone();
        section.apple_platform = form.apple_platform;
        section.apple_contacts = form.apple_contacts.clone();
        section.attachment_root = form.attachment_root.clone();
        section.conversation_filter = form.conversation_filter.clone();
    }

    /// Flush current section, switch exporter, apply the new section.
    pub fn switch_exporter(&mut self, next: Exporter, form: &mut Form) {
        if next == self.exporter {
            return;
        }
        self.capture_form_section(form);
        self.exporter = next;
        self.apply_section_to_form(form);
    }

    pub fn save(&mut self, form: &Form) -> Result<(), String> {
        self.capture_form_section(form);
        let ini = build_ini(self, form);
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("Could not create {}: {e}", parent.display())
                })?;
            }
        }
        ini.write_to_file(&self.path)
            .map_err(|e| format!("Could not write {}: {e}", self.path.display()))
    }
}

/// Prefer an existing `export.ini` in cwd, then beside the executable; otherwise cwd.
pub fn resolve_export_ini_path() -> PathBuf {
    if let Ok(dir) = env::current_dir() {
        let candidate = dir.join(EXPORT_INI_NAME);
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(EXPORT_INI_NAME);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(EXPORT_INI_NAME)
}

fn exporter_index(exporter: Exporter) -> usize {
    EXPORTERS
        .iter()
        .position(|&e| e == exporter)
        .expect("exporter in EXPORTERS")
}

fn get(ini: &Ini, section: Option<&str>, key: &str) -> String {
    ini.get_from(section, key)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn apply_common(ini: &Ini, form: &mut Form) {
    form.start_date = get(ini, Some(COMMON), "start_date");
    form.end_date = get(ini, Some(COMMON), "end_date");
    form.anonymize = parse_bool(&get(ini, Some(COMMON), "anonymize"), false);
    form.anonymize_seed = get(ini, Some(COMMON), "anonymize_seed");
    form.owner_phones = multiline_value(&get(ini, Some(COMMON), "owner_phones"));
    form.contacts = get(ini, Some(COMMON), "contacts");
    form.contacts_kind = contacts_kind_from_path(&form.contacts);

    if let Some(media) = AttachmentMedia::from_ini_str(&get(ini, Some(COMMON), "attachment_media"))
    {
        form.attachment_media = media;
    }
    if let Some(res) =
        MaxResolution::parse(&get(ini, Some(COMMON), "media_max_resolution"))
    {
        form.media_max_resolution = res;
    }
    let fps = get(ini, Some(COMMON), "media_max_fps");
    if !fps.is_empty() {
        form.media_max_fps = fps;
    }
    let min_size = get(ini, Some(COMMON), "media_min_size");
    if !min_size.is_empty() {
        form.media_min_size = min_size;
    }
    form.media_skip_efficient =
        parse_bool(&get(ini, Some(COMMON), "media_skip_efficient"), true);
}

fn read_section(ini: &Ini, exporter: Exporter) -> ExporterSection {
    let name = exporter.ini_key();
    ExporterSection {
        input: get(ini, Some(name), "input"),
        output: get(ini, Some(name), "output"),
        owner_emails: multiline_value(&get(ini, Some(name), "owner_emails")),
        name_mapping: get(ini, Some(name), "name_mapping"),
        timezone: get(ini, Some(name), "timezone"),
        apple_platform: ApplePlatform::from_ini_str(&get(ini, Some(name), "apple_platform"))
            .unwrap_or_default(),
        apple_contacts: get(ini, Some(name), "apple_contacts"),
        attachment_root: get(ini, Some(name), "attachment_root"),
        conversation_filter: get(ini, Some(name), "conversation_filter"),
    }
}

fn build_ini(state: &ExportIniState, form: &Form) -> Ini {
    let mut ini = Ini::new();
    {
        let mut common = ini.with_section(Some(COMMON));
        common
            .set("exporter", state.exporter.ini_key())
            .set("start_date", form.start_date.trim())
            .set("end_date", form.end_date.trim())
            .set("anonymize", bool_str(form.anonymize))
            .set("anonymize_seed", form.anonymize_seed.trim())
            .set("owner_phones", escape_multiline(form.owner_phones.trim()))
            .set("contacts", form.contacts.trim())
            .set("attachment_media", form.attachment_media.as_ini_str())
            .set(
                "media_max_resolution",
                form.media_max_resolution.as_str(),
            )
            .set("media_max_fps", form.media_max_fps.trim())
            .set("media_min_size", form.media_min_size.trim())
            .set(
                "media_skip_efficient",
                bool_str(form.media_skip_efficient),
            );
    }

    for exporter in EXPORTERS {
        let section = state.section(exporter);
        let mut s = ini.with_section(Some(exporter.ini_key()));
        s.set("input", section.input.trim())
            .set("output", section.output.trim());
        match exporter {
            Exporter::SmsBackupPlus => {
                s.set(
                    "owner_emails",
                    escape_multiline(section.owner_emails.trim()),
                )
                .set("name_mapping", section.name_mapping.trim());
            }
            Exporter::Imazing => {
                s.set("timezone", section.timezone.trim());
            }
            Exporter::Imessage => {
                s.set("apple_platform", section.apple_platform.as_ini_str())
                    .set("apple_contacts", section.apple_contacts.trim())
                    .set("attachment_root", section.attachment_root.trim())
                    .set(
                        "conversation_filter",
                        section.conversation_filter.trim(),
                    );
                // backup_password intentionally omitted
            }
            _ => {}
        }
    }
    ini
}

fn parse_bool(s: &str, default: bool) -> bool {
    match s.trim().to_ascii_lowercase().as_str() {
        "" => default,
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn bool_str(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}

/// Accept literal `\n` sequences from single-line INI values.
fn multiline_value(raw: &str) -> String {
    raw.replace("\\n", "\n")
}

fn escape_multiline(raw: &str) -> String {
    raw.replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn roundtrip_common_and_sections() {
        let mut form = Form {
            start_date: "2020-01-01".into(),
            end_date: "2021-01-01".into(),
            anonymize: true,
            anonymize_seed: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .into(),
            owner_phones: "+15555550100\n+15555550101".into(),
            contacts: "/tmp/contacts.vcf".into(),
            attachment_media: AttachmentMedia::Compress,
            media_max_resolution: MaxResolution::P720,
            media_max_fps: "24".into(),
            media_min_size: "10M".into(),
            media_skip_efficient: false,
            input: "/data/go".into(),
            output: "/data/go/out".into(),
            ..Form::default()
        };

        let mut state = ExportIniState {
            path: PathBuf::from("unused"),
            exporter: Exporter::GoSmsPro,
            sections: Default::default(),
        };
        state.capture_form_section(&form);
        state.switch_exporter(Exporter::SmsBackupPlus, &mut form);
        form.input = "/data/plus".into();
        form.output = "/data/plus/out".into();
        form.owner_emails = "a@example.com".into();
        form.name_mapping = "/data/map.csv".into();
        state.capture_form_section(&form);

        let mut file = NamedTempFile::new().unwrap();
        state.path = file.path().to_path_buf();
        state.save(&form).unwrap();
        file.as_file_mut().flush().unwrap();

        let (loaded, loaded_form) = ExportIniState::load(file.path()).unwrap();
        assert_eq!(loaded.exporter, Exporter::SmsBackupPlus);
        assert_eq!(loaded_form.start_date, "2020-01-01");
        assert_eq!(loaded_form.anonymize, true);
        assert_eq!(loaded_form.owner_phones, "+15555550100\n+15555550101");
        assert_eq!(loaded_form.input, "/data/plus");
        assert_eq!(loaded_form.owner_emails, "a@example.com");
        assert_eq!(
            loaded.section(Exporter::GoSmsPro).input,
            "/data/go"
        );

        // Password must never appear in the file.
        let text = fs::read_to_string(file.path()).unwrap();
        assert!(!text.contains("backup_password"));
        assert!(
            text.contains("exporter=sms-backup-plus") || text.contains("exporter = sms-backup-plus")
        );
    }

    #[test]
    fn imessage_maps_input_to_db_path() {
        let text = r#"
[common]
exporter = iphone-backup

[iphone-backup]
input = /Users/me/chat.db
output = /tmp/out
apple_platform = ios
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{text}").unwrap();
        let (state, form) = ExportIniState::load(file.path()).unwrap();
        assert_eq!(state.exporter, Exporter::Imessage);
        assert_eq!(form.db_path, "/Users/me/chat.db");
        assert!(form.input.is_empty());
        assert_eq!(form.apple_platform, ApplePlatform::Ios);
    }
}
