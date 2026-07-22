//! egui front-end for message-exporters (Validate contacts + Export).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use chrono::Local;
use eframe::egui;
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_exporters_core::{
    default_output_dir, ensure_output_dir, resolve_binary, spawn, AttachmentMedia, ContactsKind,
    Exporter, Form,
    ProcessControl, ProcessEvent, APPLE_PLATFORMS, ATTACHMENT_MEDIA, EXPORTERS,
    MAX_RESOLUTIONS,
};
use message_media::process_near_vault_media;

const LABEL_W: f32 = 190.0;
const PATH_W: f32 = 280.0;
const COMBO_W: f32 = 200.0;
const SHORT_W: f32 = 140.0;
const LOG_PLACEHOLDER: &str = "(no log output)";
const CONTACTS_FIELD_INDENT: f32 = 12.0;
/// First row plus up to 9 added rows.
const MAX_OWNER_PHONES: usize = 10;

const UTC_OFFSETS: &[&str] = &[
    "UTC-12:00",
    "UTC-11:00",
    "UTC-10:00",
    "UTC-09:30",
    "UTC-09:00",
    "UTC-08:00",
    "UTC-07:00",
    "UTC-06:00",
    "UTC-05:00",
    "UTC-04:00",
    "UTC-03:30",
    "UTC-03:00",
    "UTC-02:00",
    "UTC-01:00",
    "UTC+00:00",
    "UTC+01:00",
    "UTC+02:00",
    "UTC+03:00",
    "UTC+03:30",
    "UTC+04:00",
    "UTC+04:30",
    "UTC+05:00",
    "UTC+05:30",
    "UTC+05:45",
    "UTC+06:00",
    "UTC+06:30",
    "UTC+07:00",
    "UTC+08:00",
    "UTC+08:45",
    "UTC+09:00",
    "UTC+09:30",
    "UTC+10:00",
    "UTC+10:30",
    "UTC+11:00",
    "UTC+12:00",
    "UTC+12:45",
    "UTC+13:00",
    "UTC+14:00",
];

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, 720.0])
            .with_min_inner_size([560.0, 600.0])
            .with_title("Message Exporters"),
        ..Default::default()
    };
    eframe::run_native(
        "Message Exporters",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AppMode {
    #[default]
    ValidateContacts,
    Export,
}

struct App {
    mode: AppMode,
    exporter: Exporter,
    form: Form,
    /// Per-row owner phone inputs (always at least one). Synced into `form.owner_phones`.
    owner_phone_rows: Vec<String>,
    /// Per-row owner email inputs for SMS Backup+ (always at least one). Synced into `form.owner_emails`.
    owner_email_rows: Vec<String>,
    validate_input: String,
    validate_usa: bool,
    running: bool,
    control: ProcessControl,
    logs: Vec<String>,
    /// Selectable display buffer for the full-window log view (synced from `logs`).
    log_text: String,
    /// When true, central panel shows the scrollable log instead of the form.
    show_log: bool,
    /// Basename shown in the log header (no directory).
    session_log_name: Option<String>,
    session_log_path: Option<PathBuf>,
    errors: Vec<String>,
    rx: Option<Receiver<ProcessEvent>>,
}

impl Default for App {
    fn default() -> Self {
        let exporter = Exporter::default();
        Self {
            mode: AppMode::ValidateContacts,
            exporter,
            form: Form {
                output: default_output_dir(exporter, ""),
                ..Form::default()
            },
            owner_phone_rows: vec![String::new()],
            owner_email_rows: vec![String::new()],
            validate_input: String::new(),
            validate_usa: true,
            running: false,
            control: ProcessControl::default(),
            logs: Vec::new(),
            log_text: LOG_PLACEHOLDER.to_string(),
            show_log: false,
            session_log_name: None,
            session_log_path: None,
            errors: Vec::new(),
            rx: None,
        }
    }
}

impl App {
    fn poll_events(&mut self, ctx: &egui::Context) {
        let mut events = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(event) = rx.try_recv() {
                let done = matches!(
                    event,
                    ProcessEvent::Finished(_) | ProcessEvent::Error(_)
                );
                events.push(event);
                if done {
                    break;
                }
            }
        }
        for event in events {
            match event {
                ProcessEvent::Started(command) => {
                    self.push_log(format!("Running: {command}"));
                }
                ProcessEvent::Log(line) => self.push_log(line),
                ProcessEvent::Finished(summary) => {
                    self.push_log(summary);
                    if self.mode == AppMode::Export && self.exporter == Exporter::Imessage {
                        self.run_imessage_media_post();
                    }
                    self.running = false;
                    self.rx = None;
                }
                ProcessEvent::Error(error) => {
                    self.errors = vec![error.clone()];
                    self.push_log(format!("Error: {error}"));
                    self.running = false;
                    self.rx = None;
                }
            }
        }
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn sync_owner_phones(&mut self) {
        self.form.owner_phones = self
            .owner_phone_rows
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }

    fn sync_owner_emails(&mut self) {
        self.form.owner_emails = self
            .owner_email_rows
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }

    fn start_export(&mut self) {
        if self.running {
            return;
        }
        self.sync_owner_phones();
        self.sync_owner_emails();
        let args = match self.form.build_args(self.exporter) {
            Ok(args) => args,
            Err(errors) => {
                self.errors = errors;
                return;
            }
        };
        let output = PathBuf::from(self.form.output.trim());
        if let Err(error) = ensure_output_dir(&output) {
            self.errors = vec![error.clone()];
            self.begin_session_log();
            self.push_log(format!("Error: {error}"));
            self.show_log = true;
            return;
        }
        let program = match resolve_binary(self.exporter.binary()) {
            Ok(program) => program,
            Err(error) => {
                self.errors = vec![error];
                return;
            }
        };
        self.errors.clear();
        self.running = true;
        self.begin_session_log();
        self.show_log = true;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        spawn(program, args, self.control.clone(), tx);
    }

    fn start_validate(&mut self, check_only: bool) {
        if self.running {
            return;
        }
        let input = self.validate_input.trim();
        if input.is_empty() {
            self.errors = vec!["Choose a contacts CSV or VCF file.".into()];
            return;
        }
        if let Err(error) = message_contacts::probe_contacts_input(std::path::Path::new(input)) {
            self.errors = vec![error.message];
            return;
        }

        let program = match resolve_binary("contacts-validate") {
            Ok(program) => program,
            Err(error) => {
                self.errors = vec![error];
                return;
            }
        };
        let region = if self.validate_usa {
            "usa"
        } else {
            "international"
        };
        let mut args = vec![
            "--input".into(),
            input.into(),
            "--region".into(),
            region.into(),
        ];
        if check_only {
            args.push("--check".into());
        }
        self.errors.clear();
        self.running = true;
        self.begin_session_log();
        self.show_log = true;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        spawn(program, args, self.control.clone(), tx);
    }

    fn cancel(&mut self) {
        match self.control.cancel() {
            Ok(()) => self.push_log("Cancellation requested…".into()),
            Err(error) => self.errors = vec![error],
        }
    }

    fn run_imessage_media_post(&mut self) {
        let mode = self.form.attachment_media.media_mode();
        if matches!(mode, message_media::MediaMode::Disabled) {
            return;
        }
        let output = PathBuf::from(self.form.output.trim());
        if mode.needs_tools() {
            self.push_log(format!("Processing attachment media ({mode})…"));
            let compress = match self.form.compress_options() {
                Ok(opts) => opts,
                Err(error) => {
                    self.errors = vec![error.clone()];
                    self.push_log(format!("Error: {error}"));
                    return;
                }
            };
            match process_near_vault_media(&output, mode, &compress) {
                Ok(report) => {
                    if report.processed > 0 || report.skipped > 0 || !report.errors.is_empty() {
                        self.push_log(format!(
                            "Media: processed {} file(s), skipped {}, updated {} CSV(s)",
                            report.processed, report.skipped, report.csv_files_updated
                        ));
                    }
                    for err in report.errors.iter().take(10) {
                        self.push_log(format!("media warning: {err}"));
                    }
                }
                Err(error) => {
                    let msg = format!("Media processing failed: {error}");
                    self.errors = vec![msg.clone()];
                    self.push_log(msg);
                    return;
                }
            }
        }
        if mode.needs_tools()
            && (self.form.anonymize || !self.form.anonymize_seed.trim().is_empty())
        {
            let seed = {
                let s = self.form.anonymize_seed.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            };
            match resolve_anonymizer(seed.as_deref())
                .and_then(|mut anon| anonymize_near_vault_dir(&output, &mut anon).map(|n| (n, anon)))
            {
                Ok((n, _)) => self.push_log(format!(
                    "Anonymized {n} CSV file(s) under {}",
                    output.display()
                )),
                Err(error) => {
                    let msg = format!("Anonymize failed: {error}");
                    self.errors = vec![msg.clone()];
                    self.push_log(msg);
                }
            }
        }
    }

    fn ui_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.running, |ui| {
                if ui
                    .selectable_value(&mut self.mode, AppMode::ValidateContacts, "Contacts")
                    .clicked()
                {
                    self.show_log = false;
                }
                if ui
                    .selectable_value(&mut self.mode, AppMode::Export, "Message")
                    .clicked()
                {
                    self.show_log = false;
                }
            });
            if ui.selectable_label(self.show_log, "Log").clicked() {
                self.show_log = !self.show_log;
            }
        });
    }

    fn ui_validate(&mut self, ui: &mut egui::Ui) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.heading("Validate Contacts");
                ui.add_space(20.0);

                ui.label("Contacts file");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(CONTACTS_FIELD_INDENT);
                    if ui
                        .add_enabled(!self.running, egui::Button::new("File…"))
                        .clicked()
                    {
                        let dialog =
                            rfd::FileDialog::new().add_filter("Contacts", &["csv", "vcf", "vcard"]);
                        if let Some(path) = dialog.pick_file() {
                            self.validate_input = path.display().to_string();
                        }
                    }
                    let path_w = (ui.available_width() - 8.0).max(160.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.validate_input)
                            .id_salt("validate_contacts_file")
                            .desired_width(path_w)
                            .clip_text(true)
                            .hint_text(".vcf or .csv"),
                    );
                    if !self.validate_input.is_empty() {
                        response.on_hover_text(self.validate_input.as_str());
                    }
                });

                ui.add_space(18.0);
                ui.label("Phone number format");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(CONTACTS_FIELD_INDENT);
                    ui.vertical(|ui| {
                        ui.add_enabled_ui(!self.running, |ui| {
                            ui.radio_value(&mut self.validate_usa, true, "USA");
                            ui.radio_value(&mut self.validate_usa, false, "International");
                        });
                    });
                });

                self.ui_errors(ui);

                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.add_space(CONTACTS_FIELD_INDENT);
                    let btn_size = egui::vec2(72.0, ui.spacing().interact_size.y);
                    let check = ui.add_enabled(
                        !self.running,
                        egui::Button::new("Check").min_size(btn_size),
                    );
                    if check.clicked() {
                        self.start_validate(true);
                    }
                    let can_update = !self.running && !self.validate_input.trim().is_empty();
                    let update = ui.add_enabled(
                        can_update,
                        egui::Button::new("Update").min_size(btn_size),
                    );
                    if update.clicked() {
                        self.start_validate(false);
                    }
                    if self.running && ui.button("Cancel").clicked() {
                        self.cancel();
                    }
                });
            });
    }

    fn ui_export(&mut self, ui: &mut egui::Ui) {
        ui.heading("Export");
        ui.label(
            egui::RichText::new("Convert phone backups into readable conversation CSV").weak(),
        );
        ui.add_space(8.0);

        self.ui_backup_source(ui);
        ui.add_space(8.0);

        // Common fields (same order for every exporter).
        self.ui_common_input(ui);
        let output_hint = format!("…/{}", self.exporter.binary());
        path_or_text(
            ui,
            "Output directory",
            &mut self.form.output,
            &output_hint,
            false,
            true,
        );
        let contacts_enabled = self.exporter != Exporter::Imessage;
        let attachments_enabled = matches!(
            self.exporter,
            Exporter::GoSmsPro
                | Exporter::SmsBackupRestore
                | Exporter::SmsBackupPlus
                | Exporter::Imazing
                | Exporter::Imessage
        );
        if self.exporter == Exporter::OpenExtract {
            self.form.attachment_media = AttachmentMedia::Disabled;
        }
        self.ui_contacts(ui, contacts_enabled);
        self.ui_attachment_media(ui, attachments_enabled);

        // Exporter-specific fields.
        match self.exporter {
            Exporter::GoSmsPro | Exporter::SmsBackupRestore => {
                self.ui_owner_phones(ui);
            }
            Exporter::SmsBackupPlus => {
                self.ui_owner_phones(ui);
                self.ui_owner_emails(ui);
                path_or_text(
                    ui,
                    "Name mapping",
                    &mut self.form.name_mapping,
                    ".csv",
                    true,
                    false,
                );
            }
            Exporter::Imazing => {
                self.ui_timezone(ui);
            }
            Exporter::OpenExtract => {}
            Exporter::Imessage => {
                ui.horizontal(|ui| {
                    ui.allocate_exact_size(
                        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
                        egui::Sense::hover(),
                    );
                    if ui
                        .button(if self.form.advanced {
                            "▾ Hide advanced options"
                        } else {
                            "▸ Show advanced options"
                        })
                        .clicked()
                    {
                        self.form.advanced = !self.form.advanced;
                    }
                });
                if self.form.advanced {
                    combo_enum(
                        ui,
                        "Platform",
                        &mut self.form.apple_platform,
                        &APPLE_PLATFORMS,
                        PATH_W,
                    );
                    ui.horizontal(|ui| {
                        form_label(ui, "Backup password");
                        with_field_width(ui, PATH_W, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.form.backup_password)
                                    .password(true)
                                    .desired_width(PATH_W)
                                    .clip_text(true)
                                    .hint_text("Encrypted iOS backup password"),
                            );
                        });
                    });
                    path_or_text(
                        ui,
                        "Apple AddressBook DB",
                        &mut self.form.apple_contacts,
                        "Path",
                        true,
                        false,
                    );
                    path_or_text(
                        ui,
                        "Attachment root",
                        &mut self.form.attachment_root,
                        "Path",
                        false,
                        true,
                    );
                    path_or_text(
                        ui,
                        "Conversation filter",
                        &mut self.form.conversation_filter,
                        "Names, numbers, or emails (comma-separated)",
                        false,
                        false,
                    );
                }
            }
        }

        self.ui_errors(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.heading(egui::RichText::new("Message filtering").size(16.0));
        labeled_text(
            ui,
            "Start date",
            &mut self.form.start_date,
            "YYYY-MM-DD",
            PATH_W,
        );
        labeled_text(
            ui,
            "End date",
            &mut self.form.end_date,
            "YYYY-MM-DD (exclusive)",
            PATH_W,
        );
        ui.horizontal(|ui| {
            form_label(ui, "Anonymize");
            let anonymize_text = if self.form.anonymize { "Yes" } else { "No" };
            with_field_width(ui, PATH_W, |ui| {
                egui::ComboBox::from_id_salt("anonymize")
                    .selected_text(anonymize_text)
                    .width(PATH_W)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.form.anonymize, false, "No");
                        ui.selectable_value(&mut self.form.anonymize, true, "Yes");
                    });
            });
        });
        if self.form.anonymize || !self.form.anonymize_seed.is_empty() {
            labeled_text(
                ui,
                "Seed",
                &mut self.form.anonymize_seed,
                "Optional 64-hex seed",
                PATH_W,
            );
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let run = ui.add_enabled(!self.running, egui::Button::new("Run exporter"));
            if run.clicked() {
                self.start_export();
            }
        });
    }

    fn ui_backup_source(&mut self, ui: &mut egui::Ui) {
        let previous = self.exporter;
        ui.horizontal(|ui| {
            form_label(ui, "Backup type");
            with_field_width(ui, PATH_W, |ui| {
                egui::ComboBox::from_id_salt("exporter")
                    .selected_text(self.exporter.display_name())
                    .width(PATH_W)
                    .show_ui(ui, |ui| {
                        for exporter in EXPORTERS {
                            ui.selectable_value(
                                &mut self.exporter,
                                exporter,
                                exporter.display_name(),
                            );
                        }
                    });
            });
            let link_text = format!("↗ {}", self.exporter.link_label());
            if ui
                .link(link_text)
                .on_hover_text(self.exporter.product_url())
                .clicked()
            {
                if let Err(error) = open::that(self.exporter.product_url()) {
                    self.errors = vec![format!("Could not open link: {error}")];
                }
            }
        });
        if self.exporter != previous {
            let previous_input = if previous == Exporter::Imessage {
                self.form.db_path.as_str()
            } else {
                self.form.input.as_str()
            };
            let previous_default = default_output_dir(previous, previous_input);
            if self.form.output.trim().is_empty() || self.form.output == previous_default {
                let new_input = if self.exporter == Exporter::Imessage {
                    self.form.db_path.as_str()
                } else {
                    self.form.input.as_str()
                };
                self.form.output = default_output_dir(self.exporter, new_input);
            }
            self.form.advanced = false;
            self.errors.clear();
        }
    }

    fn sync_default_output(&mut self, old_input: &str, new_input: &str) {
        let previous_default = default_output_dir(self.exporter, old_input);
        if self.form.output.trim().is_empty() || self.form.output == previous_default {
            self.form.output = default_output_dir(self.exporter, new_input);
        }
    }

    fn ui_common_input(&mut self, ui: &mut egui::Ui) {
        if self.exporter == Exporter::Imessage {
            let old_db = self.form.db_path.clone();
            path_or_text(
                ui,
                "Database / iOS backup path",
                &mut self.form.db_path,
                "Path",
                true,
                true,
            );
            if self.form.db_path != old_db {
                self.sync_default_output(&old_db, &self.form.db_path.clone());
            }
            return;
        }
        let (file, folder) = match self.exporter {
            Exporter::GoSmsPro | Exporter::Imazing => (false, true),
            Exporter::SmsBackupRestore
            | Exporter::SmsBackupPlus
            | Exporter::OpenExtract => (true, true),
            Exporter::Imessage => unreachable!(),
        };
        let input_label = if self.exporter == Exporter::SmsBackupPlus {
            "Input file or folder"
        } else {
            "Input directory"
        };
        let old_input = self.form.input.clone();
        path_or_text(ui, input_label, &mut self.form.input, "Path", file, folder);
        if self.form.input != old_input {
            self.sync_default_output(&old_input, &self.form.input.clone());
        }
    }

    fn ui_timezone(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            form_label(ui, "Timezone");
            let selected = if self.form.timezone.trim().is_empty() {
                "Local time".to_string()
            } else {
                self.form.timezone.clone()
            };
            with_field_width(ui, PATH_W, |ui| {
                egui::ComboBox::from_id_salt("timezone")
                    .selected_text(selected)
                    .width(PATH_W)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.form.timezone.trim().is_empty(), "Local time")
                            .clicked()
                        {
                            self.form.timezone.clear();
                        }
                        for offset in UTC_OFFSETS {
                            if ui
                                .selectable_label(self.form.timezone == *offset, *offset)
                                .clicked()
                            {
                                self.form.timezone = (*offset).to_string();
                            }
                        }
                    });
            });
        });
    }

    fn ui_owner_phones(&mut self, ui: &mut egui::Ui) {
        if self.owner_phone_rows.is_empty() {
            self.owner_phone_rows.push(String::new());
        }
        let mut remove_idx = None;
        let mut add_row = false;
        let row_count = self.owner_phone_rows.len();
        for i in 0..row_count {
            ui.horizontal(|ui| {
                if i == 0 {
                    form_label(ui, "Your phone number(s)");
                } else {
                    ui.allocate_exact_size(
                        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
                        egui::Sense::hover(),
                    );
                }
                with_field_width(ui, PATH_W, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.owner_phone_rows[i])
                            .id_salt(("owner_phone", i))
                            .desired_width(PATH_W)
                            .clip_text(true)
                            .hint_text("+19995551234"),
                    );
                });
                if i == 0 {
                    let can_add = row_count < MAX_OWNER_PHONES;
                    let add = ui
                        .add_enabled(can_add, egui::Button::new("+"))
                        .on_hover_text(if can_add {
                            "Add phone number"
                        } else {
                            "Maximum of 10 phone numbers"
                        });
                    if add.clicked() {
                        add_row = true;
                    }
                } else if ui
                    .button("−")
                    .on_hover_text("Remove phone number")
                    .clicked()
                {
                    remove_idx = Some(i);
                }
            });
        }
        if add_row && self.owner_phone_rows.len() < MAX_OWNER_PHONES {
            self.owner_phone_rows.push(String::new());
        }
        if let Some(i) = remove_idx {
            if i > 0 && i < self.owner_phone_rows.len() {
                self.owner_phone_rows.remove(i);
            }
        }
        self.sync_owner_phones();
    }

    fn ui_owner_emails(&mut self, ui: &mut egui::Ui) {
        if self.owner_email_rows.is_empty() {
            self.owner_email_rows.push(String::new());
        }
        let mut remove_idx = None;
        let mut add_row = false;
        let row_count = self.owner_email_rows.len();
        for i in 0..row_count {
            ui.horizontal(|ui| {
                if i == 0 {
                    form_label(ui, "Backup email address");
                } else {
                    ui.allocate_exact_size(
                        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
                        egui::Sense::hover(),
                    );
                }
                with_field_width(ui, PATH_W, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.owner_email_rows[i])
                            .id_salt(("owner_email", i))
                            .desired_width(PATH_W)
                            .clip_text(true)
                            .hint_text("you@example.com"),
                    );
                });
                if i == 0 {
                    let can_add = row_count < MAX_OWNER_PHONES;
                    let add = ui
                        .add_enabled(can_add, egui::Button::new("+"))
                        .on_hover_text(if can_add {
                            "Add email address"
                        } else {
                            "Maximum of 10 email addresses"
                        });
                    if add.clicked() {
                        add_row = true;
                    }
                } else if ui
                    .button("−")
                    .on_hover_text("Remove email address")
                    .clicked()
                {
                    remove_idx = Some(i);
                }
            });
        }
        if add_row && self.owner_email_rows.len() < MAX_OWNER_PHONES {
            self.owner_email_rows.push(String::new());
        }
        if let Some(i) = remove_idx {
            if i > 0 && i < self.owner_email_rows.len() {
                self.owner_email_rows.remove(i);
            }
        }
        self.sync_owner_emails();
    }

    fn ui_contacts(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            path_or_text(
                ui,
                "Contacts file",
                &mut self.form.contacts,
                ".csv or .vcf",
                true,
                false,
            );
        });
        if enabled {
            self.form.contacts_kind = if self.form.contacts.trim().is_empty() {
                ContactsKind::None
            } else {
                ContactsKind::Csv
            };
        }
    }

    fn ui_attachment_media(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            combo_enum(
                ui,
                "Attachments",
                &mut self.form.attachment_media,
                &ATTACHMENT_MEDIA,
                PATH_W,
            );
            if self.form.attachment_media.needs_ffmpeg() && !message_media::ffmpeg_available() {
                ui.colored_label(
                    egui::Color32::from_rgb(180, 50, 50),
                    "Convert/Compress need ffmpeg and ffprobe on PATH.",
                );
            }
            if self.form.attachment_media == AttachmentMedia::Compress {
                combo_enum(
                    ui,
                    "Max resolution",
                    &mut self.form.media_max_resolution,
                    &MAX_RESOLUTIONS,
                    COMBO_W,
                );
                labeled_text(ui, "Max fps", &mut self.form.media_max_fps, "e.g. 30", SHORT_W);
                labeled_text(
                    ui,
                    "Min size",
                    &mut self.form.media_min_size,
                    "e.g. 20M",
                    SHORT_W,
                );
                ui.checkbox(
                    &mut self.form.media_skip_efficient,
                    "Skip already-efficient HEVC",
                );
            }
        });
    }

    fn ui_errors(&self, ui: &mut egui::Ui) {
        if self.errors.is_empty() {
            return;
        }
        ui.add_space(8.0);
        egui::Frame::NONE
            .fill(egui::Color32::from_rgb(255, 235, 235))
            .stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgb(200, 80, 80),
            ))
            .inner_margin(10.0)
            .corner_radius(6.0)
            .show(ui, |ui| {
                for error in &self.errors {
                    ui.colored_label(
                        egui::Color32::from_rgb(140, 40, 40),
                        format!("• {error}"),
                    );
                }
            });
    }

    fn status_text(&self) -> String {
        if let Some(last) = self.logs.last() {
            return last.clone();
        }
        if self.running {
            return "Running…".into();
        }
        String::new()
    }

    fn sync_log_text(&mut self) {
        self.log_text = if self.logs.is_empty() {
            LOG_PLACEHOLDER.to_string()
        } else {
            self.logs.join("\n")
        };
    }

    fn ensure_session_log(&mut self) {
        if self.session_log_path.is_some() {
            return;
        }
        let (name, path) = new_session_log_file();
        self.session_log_name = Some(name);
        self.session_log_path = Some(path);
    }

    /// Start (or reset) the current session log file and clear the in-UI buffer.
    fn begin_session_log(&mut self) {
        self.ensure_session_log();
        self.logs.clear();
        if let Some(path) = &self.session_log_path {
            let _ = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path);
        }
        self.sync_log_text();
    }

    fn push_log(&mut self, line: String) {
        self.ensure_session_log();
        if let Some(path) = &self.session_log_path {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{line}");
            }
        }
        self.logs.push(line);
        self.sync_log_text();
    }

    fn ui_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let status = self.status_text();
            if !status.is_empty() {
                ui.label(egui::RichText::new(&status).weak().small())
                    .on_hover_text(&status);
            }
        });
    }

    fn ui_log_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let name = self.session_log_name.as_deref().unwrap_or("(log)");
            ui.label(egui::RichText::new(name).strong());
            if !self.logs.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Clear").clicked() {
                        self.logs.clear();
                        self.sync_log_text();
                    }
                });
            }
        });

        ui.add_space(4.0);
        let body_height = ui.available_height().max(80.0);
        egui::ScrollArea::vertical()
            .id_salt("export_log_scroll")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .max_height(body_height)
            .min_scrolled_height(body_height)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                let line_count = self.log_text.lines().count().max(1) as f32;
                let content_height = (line_count * row_height + 8.0).max(body_height);
                // Immutable &str TextBuffer: select/copy work; typing cannot mutate.
                let mut readonly: &str = self.log_text.as_str();
                ui.add_sized(
                    [ui.available_width(), content_height],
                    egui::TextEdit::multiline(&mut readonly)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .interactive(true),
                );
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events(ctx);
        self.sync_log_text();

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            self.ui_tabs(ui);
            ui.add_space(2.0);
        });

        egui::TopBottomPanel::bottom("status")
            .exact_height(28.0)
            .show_separator_line(true)
            .show(ctx, |ui| {
                self.ui_status_bar(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.show_log {
                ui.set_min_size(ui.available_size());
                self.ui_log_panel(ui);
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.mode {
                        AppMode::ValidateContacts => self.ui_validate(ui),
                        AppMode::Export => self.ui_export(ui),
                    }
                });
            }
        });
    }
}

fn new_session_log_file() -> (String, PathBuf) {
    let name = Local::now()
        .format("message-exporters-%Y-%m-%d_%H%M%S.log")
        .to_string();
    let path = std::env::temp_dir().join(&name);
    let _ = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path);
    (name, path)
}

fn form_label(ui: &mut egui::Ui, label: &str) {
    // Fixed LABEL_W column (keeps fields aligned); right-to-left packs label against the inputs.
    ui.allocate_ui_with_layout(
        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.add(egui::Label::new(label).truncate());
        },
    );
}

/// Reserve an exact field width so trailing buttons cannot shrink the control.
fn with_field_width(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        add,
    );
}

fn labeled_text(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str, width: f32) {
    ui.horizontal(|ui| {
        form_label(ui, label);
        let mut response = None;
        with_field_width(ui, width, |ui| {
            response = Some(
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(width)
                        .clip_text(true)
                        .hint_text(hint),
                ),
            );
        });
        if let Some(response) = response {
            if !value.is_empty() {
                response.on_hover_text(value.as_str());
            }
        }
    });
}

fn path_or_text(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    allow_file: bool,
    allow_folder: bool,
) {
    ui.horizontal(|ui| {
        form_label(ui, label);
        let mut response = None;
        with_field_width(ui, PATH_W, |ui| {
            response = Some(
                ui.add(
                    egui::TextEdit::singleline(value)
                        .id_salt(label)
                        .desired_width(PATH_W)
                        .clip_text(true)
                        .hint_text(hint),
                ),
            );
        });
        if let Some(response) = response {
            if !value.is_empty() {
                response.on_hover_text(value.as_str());
            }
        }
        if allow_file
            && ui
                .button("📄")
                .on_hover_text("Choose file…")
                .clicked()
        {
            let mut dialog = rfd::FileDialog::new();
            if label.to_ascii_lowercase().contains("contact") {
                dialog = dialog.add_filter("Contacts", &["csv", "vcf", "vcard"]);
            }
            if let Some(path) = dialog.pick_file() {
                *value = path.display().to_string();
            }
        }
        if allow_folder
            && ui
                .button("📁")
                .on_hover_text("Choose folder…")
                .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                *value = path.display().to_string();
            }
        }
    });
}

fn combo_enum<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[T],
    width: f32,
) {
    ui.horizontal(|ui| {
        form_label(ui, label);
        with_field_width(ui, width, |ui| {
            egui::ComboBox::from_id_salt(label)
                .selected_text(value.to_string())
                .width(width)
                .show_ui(ui, |ui| {
                    for opt in options {
                        ui.selectable_value(value, *opt, opt.to_string());
                    }
                });
        });
    });
}
