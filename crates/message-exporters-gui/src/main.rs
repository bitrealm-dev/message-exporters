//! egui front-end for message-exporters (Validate contacts + Export).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use chrono::Local;
use eframe::egui;
use go_sms_pro_exporter::run as run_go_sms_pro;
use imazing_exporter::run as run_imazing;
use imessage_ir_exporter::run as run_imessage;
use message_exporters_core::{
    ensure_output_dir, resolve_binary, spawn, spawn_job, AttachmentMedia, ContactsKind,
    ExportIniState, Exporter, ExporterConfig, Form, MediaConfig, MessageReexportConfig,
    ObfuscateConfig, ProcessControl, ProcessEvent, SourceConfig, WhatsappPlatform,
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, EXPORTERS, MAX_RESOLUTIONS, OUTPUT_FORMATS_MAIL,
    WHATSAPP_PLATFORMS,
};
use message_reexporter::run as run_reexport;
use openextract_exporter::run as run_openextract;
use sms_backup_plus_exporter::run as run_sms_plus;
use sms_backup_restore_exporter::run as run_sms_restore;
use whatsapp_exporter::run as run_whatsapp;

const LABEL_W: f32 = 190.0;
const PATH_W: f32 = 400.0;
const COMBO_W: f32 = 200.0;
const SHORT_W: f32 = 140.0;
const MIN_FIELD_W: f32 = 160.0;
const PICKER_BUTTON_W: f32 = 72.0;
const LOG_PLACEHOLDER: &str = "(no log output)";
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
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([680.0, 480.0])
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
    Reexport,
    Log,
}

struct App {
    mode: AppMode,
    exporter: Exporter,
    form: Form,
    export_ini: ExportIniState,
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
    /// Basename shown in the log header (no directory).
    session_log_name: Option<String>,
    session_log_path: Option<PathBuf>,
    errors: Vec<String>,
    rx: Option<Receiver<ProcessEvent>>,
}

impl Default for App {
    fn default() -> Self {
        let (export_ini, form) = ExportIniState::load_or_default();
        let exporter = export_ini.exporter;
        let owner_phone_rows = rows_from_multiline(&form.owner_phones);
        let owner_email_rows = rows_from_multiline(&form.owner_emails);
        Self {
            mode: AppMode::ValidateContacts,
            exporter,
            form,
            export_ini,
            owner_phone_rows,
            owner_email_rows,
            validate_input: String::new(),
            validate_usa: true,
            running: false,
            control: ProcessControl::default(),
            logs: Vec::new(),
            log_text: LOG_PLACEHOLDER.to_string(),
            session_log_name: None,
            session_log_path: None,
            errors: Vec::new(),
            rx: None,
        }
    }
}

fn rows_from_multiline(value: &str) -> Vec<String> {
    let rows: Vec<String> = value
        .split(|c| c == '\n' || c == ',' || c == ';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if rows.is_empty() {
        vec![String::new()]
    } else {
        rows
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

    fn persist_export_ini(&mut self) {
        self.sync_owner_phones();
        self.sync_owner_emails();
        self.export_ini.exporter = self.exporter;
        if let Err(error) = self.export_ini.save(&self.form) {
            self.errors = vec![error];
        }
    }

    /// Reset the active exporter's form fields and wipe its INI section.
    fn clear_active_exporter(&mut self) {
        self.sync_owner_phones();
        self.sync_owner_emails();
        self.export_ini.exporter = self.exporter;
        self.export_ini.clear_active_section(&mut self.form);
        self.owner_email_rows = rows_from_multiline(&self.form.owner_emails);
        self.errors.clear();
        if let Err(error) = self.export_ini.save(&self.form) {
            self.errors = vec![error];
        }
    }

    fn start_reexport(&mut self) {
        if self.running {
            return;
        }
        let _ = self.export_ini.save(&self.form);

        let mut errors = Vec::new();
        let input = self.export_ini.reexport.input.trim().to_string();
        let output = self.export_ini.reexport.output.trim().to_string();
        if input.is_empty() {
            errors.push("Input directory is required.".into());
        } else if !PathBuf::from(&input).is_dir() {
            errors.push(format!("Input directory does not exist: {input}"));
        }
        if output.is_empty() {
            errors.push("Output directory is required.".into());
        }
        if self.form.attachment_media.needs_ffmpeg() && !message_media::ffmpeg_available() {
            errors.push("Convert/Compress require ffmpeg and ffprobe on PATH.".into());
        }
        let seed = self.form.obfuscate_seed.trim();
        let obfuscate_seed = if seed.is_empty() {
            None
        } else if seed.len() == 8 && seed.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(seed.to_string())
        } else {
            errors.push("Obfuscate seed must be exactly 8 hex characters.".into());
            None
        };
        let compress = if matches!(
            self.form.attachment_media,
            AttachmentMedia::Compress
        ) {
            match self.form.compress_options() {
                Ok(options) => options,
                Err(error) => {
                    errors.push(error);
                    message_media::CompressOptions::default()
                }
            }
        } else {
            message_media::CompressOptions::default()
        };
        if !errors.is_empty() {
            self.errors = errors;
            return;
        }

        let output_path = PathBuf::from(&output);
        if let Err(error) = ensure_output_dir(&output_path) {
            self.errors = vec![error];
            return;
        }

        let config = ExporterConfig {
            inputs: vec![PathBuf::from(input)],
            output: output_path,
            date_range: Default::default(),
            contacts: None,
            obfuscate: ObfuscateConfig {
                enabled: self.form.obfuscate || obfuscate_seed.is_some(),
                seed: obfuscate_seed,
            },
            media: MediaConfig {
                mode: self.form.attachment_media.media_mode(),
                compress,
            },
            cancel: None,
            output_format: self.export_ini.reexport.output_format,
            source: SourceConfig::MessageReexport(MessageReexportConfig {}),
        };

        let label = "message-reexporter (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_reexport(&config), tx)
        });

        self.errors.clear();
        self.running = true;
        self.begin_session_log();
        self.mode = AppMode::Log;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        spawn_job(self.control.clone(), tx, label, job);
    }

    fn start_export(&mut self) {
        if self.running {
            return;
        }
        self.sync_owner_phones();
        self.sync_owner_emails();
        self.export_ini.exporter = self.exporter;
        let _ = self.export_ini.save(&self.form);
        let config = match self.form.to_config(self.exporter) {
            Ok(config) => config,
            Err(errors) => {
                self.errors = errors;
                return;
            }
        };
        let output = PathBuf::from(self.form.output.trim());
        if let Err(error) = ensure_output_dir(&output) {
            self.errors = vec![error];
            return;
        }

        let label = format!("{} (library)", self.exporter.binary());
        let job = library_job_for_exporter(self.exporter, config);

        self.errors.clear();
        self.running = true;
        self.begin_session_log();
        self.mode = AppMode::Log;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        spawn_job(self.control.clone(), tx, label, job);
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
        self.mode = AppMode::Log;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        spawn(program, args, self.control.clone(), tx);
    }

    fn cancel(&mut self) {
        match self.control.cancel() {
            Ok(()) => self.push_log("Cancellation requested…".into()),
            Err(error) => {
                self.errors = vec![error.clone()];
                self.push_log(format!("Could not request cancellation: {error}"));
            }
        }
    }

    fn ui_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.running, |ui| {
                ui.selectable_value(&mut self.mode, AppMode::ValidateContacts, "Contacts");
                ui.selectable_value(&mut self.mode, AppMode::Export, "Message");
                ui.selectable_value(&mut self.mode, AppMode::Reexport, "Re-export");
            });
            ui.selectable_value(&mut self.mode, AppMode::Log, "Log");
        });
    }

    fn ui_validate(&mut self, ui: &mut egui::Ui) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.heading("Validate Contacts");
                required_field_note(ui);
                ui.add_space(6.0);

                let contacts_label = required_field_label(ui, "Contacts file");
                path_or_text_labeled(
                    ui,
                    contacts_label,
                    "validate_contacts_file",
                    &mut self.validate_input,
                    ".vcf or .csv",
                    true,
                    false,
                );
                ui.horizontal(|ui| {
                    form_label(ui, "Phone number format");
                    ui.vertical(|ui| {
                        ui.radio_value(&mut self.validate_usa, true, "USA");
                        ui.radio_value(&mut self.validate_usa, false, "International");
                    });
                });

                self.ui_errors(ui);

                ui.add_space(16.0);
                form_action_row(ui, |ui| {
                    let can_validate = !self.validate_input.trim().is_empty();
                    let check = form_action_button(ui, "Check", can_validate);
                    if check.clicked() {
                        self.start_validate(true);
                    }
                    let update = form_action_button(ui, "Update", can_validate);
                    if update.clicked() {
                        self.start_validate(false);
                    }
                });
            });
    }

    fn ui_export(&mut self, ui: &mut egui::Ui) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| self.ui_export_content(ui));
    }

    fn ui_export_content(&mut self, ui: &mut egui::Ui) {
        ui.heading("Export");
        required_field_note(ui);
        ui.add_space(6.0);

        self.ui_backup_source(ui);
        self.ui_output_format(ui);
        ui.add_space(8.0);

        if self.exporter == Exporter::Whatsapp {
            combo_enum(
                ui,
                "Platform",
                &mut self.form.whatsapp_platform,
                &WHATSAPP_PLATFORMS,
                PATH_W,
            );
        }

        // Common fields. WhatsApp has no Input / Contacts file; its source fields
        // come before Attachments (see below).
        if self.exporter != Exporter::Whatsapp {
            self.ui_common_input(ui);
            required_path_or_text(
                ui,
                "Output directory",
                &mut self.form.output,
                "Path",
                false,
                true,
            );
        }

        // WhatsApp: Output format → Platform → backup / contacts → Output → Attachments → Advanced.
        if self.exporter == Exporter::Whatsapp {
            if self.form.whatsapp_platform == WhatsappPlatform::Ios {
                required_path_or_text(
                    ui,
                    "Backup path",
                    &mut self.form.whatsapp_backup,
                    "MobileSync Backup/DEVICE_ID folder",
                    false,
                    true,
                );
                path_or_text(
                    ui,
                    "Contacts (optional)",
                    &mut self.form.whatsapp_wa,
                    "ContactsV2.sqlite",
                    true,
                    false,
                );
            } else {
                path_or_text(
                    ui,
                    "Backup path (optional)",
                    &mut self.form.whatsapp_backup,
                    "msgstore.db.crypt12 / .crypt14 / .crypt15",
                    true,
                    true,
                );
                path_or_text(
                    ui,
                    "Contacts (optional)",
                    &mut self.form.whatsapp_wa,
                    "wa.db",
                    true,
                    false,
                );
            }
            required_path_or_text(
                ui,
                "Output directory",
                &mut self.form.output,
                "Path",
                false,
                true,
            );
        }

        let contacts_enabled = self.exporter != Exporter::Imessage;
        if self.exporter != Exporter::Whatsapp {
            self.ui_contacts(ui, contacts_enabled);
        }
        // Attachment modes (none / copy / convert / compress) apply to every format via FormatSink.
        self.ui_attachment_media(ui, true);

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
                    "Name mapping (optional)",
                    &mut self.form.name_mapping,
                    "Phone,Incorrect Name CSV",
                    true,
                    false,
                );
            }
            Exporter::Imazing => {
                self.ui_timezone(ui);
            }
            Exporter::Whatsapp => {
                if self.form.whatsapp_platform == WhatsappPlatform::Android {
                    labeled_text(
                        ui,
                        "Decryption key",
                        &mut self.form.whatsapp_key,
                        "Key file or crypt15 hex",
                        PATH_W,
                    );
                }
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
                    if self.form.whatsapp_platform == WhatsappPlatform::Android {
                        path_or_text(
                            ui,
                            "Media folder (optional)",
                            &mut self.form.whatsapp_media,
                            "WhatsApp media directory",
                            false,
                            true,
                        );
                        path_or_text(
                            ui,
                            "Message database (optional)",
                            &mut self.form.whatsapp_db,
                            "msgstore.db override",
                            true,
                            false,
                        );
                    }
                    ui.horizontal(|ui| {
                        form_label(ui, "WhatsApp Business");
                        ui.checkbox(&mut self.form.whatsapp_business, "");
                    });
                }
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
                        let width = responsive_field_width(ui, PATH_W, 0);
                        form_label(ui, "Backup password (optional)");
                        with_field_width(ui, width, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.form.backup_password)
                                    .password(true)
                                    .desired_width(width)
                                    .clip_text(true)
                                    .hint_text("Encrypted iOS backup password"),
                            );
                        });
                    });
                    path_or_text(
                        ui,
                        "Apple AddressBook DB (optional)",
                        &mut self.form.apple_contacts,
                        "Path",
                        true,
                        false,
                    );
                    path_or_text(
                        ui,
                        "Attachment root (optional)",
                        &mut self.form.attachment_root,
                        "Path",
                        false,
                        true,
                    );
                    path_or_text(
                        ui,
                        "Conversation filter (optional)",
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
        ui.heading(egui::RichText::new("Message filtering (optional)").size(16.0));
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
            form_label(ui, "Obfuscate");
            ui.checkbox(&mut self.form.obfuscate, "");
        });
        if self.form.obfuscate || !self.form.obfuscate_seed.is_empty() {
            labeled_text(
                ui,
                "Seed (optional)",
                &mut self.form.obfuscate_seed,
                "8-hex seed",
                PATH_W,
            );
        }
        ui.add_space(10.0);
        form_action_row(ui, |ui| {
            let run = form_action_button(ui, "Run exporter", true);
            if run.clicked() {
                self.start_export();
            }
            let clear = form_action_button(ui, "Clear", true).on_hover_text(format!(
                    "Clear {} fields and remove them from {}",
                    self.exporter.display_name(),
                    self.export_ini.path.display()
                ));
            if clear.clicked() {
                self.clear_active_exporter();
            }
        });
    }

    fn ui_reexport(&mut self, ui: &mut egui::Ui) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(18))
            .show(ui, |ui| {
                ui.heading("Re-export");
                required_field_note(ui);
                ui.add_space(6.0);
                ui.label(
                    "Convert a prior Message Exporters output directory to another format. \
                     Input format is auto-detected (csv, eml, mbox, json, jsonl, or xml).",
                );
                ui.add_space(16.0);

                required_path_or_text(
                    ui,
                    "Input directory",
                    &mut self.export_ini.reexport.input,
                    "Prior export folder",
                    false,
                    true,
                );
                self.ui_reexport_output_format(ui);
                required_path_or_text(
                    ui,
                    "Output directory",
                    &mut self.export_ini.reexport.output,
                    "Path",
                    false,
                    true,
                );
                self.ui_attachment_media(ui, true);

                self.ui_errors(ui);

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    form_label(ui, "Obfuscate");
                    ui.checkbox(&mut self.form.obfuscate, "");
                });
                if self.form.obfuscate || !self.form.obfuscate_seed.is_empty() {
                    labeled_text(
                        ui,
                        "Seed (optional)",
                        &mut self.form.obfuscate_seed,
                        "8-hex seed",
                        PATH_W,
                    );
                }
                ui.add_space(10.0);
                form_action_row(ui, |ui| {
                    let run = form_action_button(ui, "Run re-export", true);
                    if run.clicked() {
                        self.start_reexport();
                    }
                    let clear = form_action_button(ui, "Clear", true);
                    if clear.clicked() {
                        self.export_ini.reexport = Default::default();
                        let _ = self.export_ini.save(&self.form);
                    }
                });
            });
    }

    fn ui_reexport_output_format(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let width = responsive_field_width(ui, PATH_W, 0);
            form_label(ui, "Output format");
            with_field_width(ui, width, |ui| {
                egui::ComboBox::from_id_salt("reexport_output_format")
                    .selected_text(self.export_ini.reexport.output_format.to_string())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for format in OUTPUT_FORMATS_MAIL {
                            ui.selectable_value(
                                &mut self.export_ini.reexport.output_format,
                                format,
                                format.to_string(),
                            );
                        }
                    });
            });
        });
    }

    fn ui_backup_source(&mut self, ui: &mut egui::Ui) {
        let previous = self.exporter;
        ui.horizontal(|ui| {
            let width = responsive_field_width(ui, PATH_W, 0);
            form_label(ui, "Backup type");
            with_field_width(ui, width, |ui| {
                egui::ComboBox::from_id_salt("exporter")
                    .selected_text(self.exporter.dropdown_label())
                    .width(width)
                    .show_ui(ui, |ui| {
                        let mut saw_experimental = false;
                        for exporter in EXPORTERS {
                            if !exporter.is_supported() && !saw_experimental {
                                ui.separator();
                                saw_experimental = true;
                            }
                            ui.selectable_value(
                                &mut self.exporter,
                                exporter,
                                exporter.dropdown_label(),
                            );
                        }
                    });
            });
        });
        form_action_row(ui, |ui| {
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
            self.sync_owner_phones();
            self.sync_owner_emails();
            self.export_ini
                .switch_exporter(self.exporter, &mut self.form);
            self.owner_email_rows = rows_from_multiline(&self.form.owner_emails);
            self.form.advanced = false;
            self.errors.clear();
        }
    }

    fn ui_common_input(&mut self, ui: &mut egui::Ui) {
        if self.exporter == Exporter::Imessage {
            path_or_text(
                ui,
                "Database / iOS backup path (optional)",
                &mut self.form.db_path,
                "Path",
                true,
                true,
            );
            return;
        }
        let (file, folder) = match self.exporter {
            Exporter::GoSmsPro | Exporter::Imazing | Exporter::Whatsapp => (false, true),
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
        required_path_or_text(ui, input_label, &mut self.form.input, "Path", file, folder);
    }

    fn ui_timezone(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let width = responsive_field_width(ui, PATH_W, 0);
            form_label(ui, "Timezone (optional)");
            let selected = if self.form.timezone.trim().is_empty() {
                "Local time".to_string()
            } else {
                self.form.timezone.clone()
            };
            with_field_width(ui, width, |ui| {
                egui::ComboBox::from_id_salt("timezone")
                    .selected_text(selected)
                    .width(width)
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
                let width = responsive_field_width(ui, PATH_W, 1);
                if i == 0 {
                    let label = required_field_label(ui, "Your phone number(s)");
                    form_label(ui, label);
                } else {
                    ui.allocate_exact_size(
                        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
                        egui::Sense::hover(),
                    );
                }
                with_field_width(ui, width, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.owner_phone_rows[i])
                            .id_salt(("owner_phone", i))
                            .desired_width(width)
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
                let width = responsive_field_width(ui, PATH_W, 1);
                if i == 0 {
                    let label = required_field_label(ui, "Backup email address");
                    form_label(ui, label);
                } else {
                    ui.allocate_exact_size(
                        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
                        egui::Sense::hover(),
                    );
                }
                with_field_width(ui, width, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.owner_email_rows[i])
                            .id_salt(("owner_email", i))
                            .desired_width(width)
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
            // iPhone backup: keep the row for layout, but show an empty field so a
            // shared [common] contacts path does not look like it applies here.
            if enabled {
                path_or_text_labeled(
                    ui,
                    "Contacts file (optional)",
                    "Contacts file",
                    &mut self.form.contacts,
                    ".csv or .vcf",
                    true,
                    false,
                );
            } else {
                let mut blank = String::new();
                path_or_text_labeled(
                    ui,
                    "Contacts file",
                    "Contacts file",
                    &mut blank,
                    "Not used — set Apple AddressBook under Advanced",
                    true,
                    false,
                );
            }
        });
        if enabled {
            let path = self.form.contacts.trim();
            self.form.contacts_kind = if path.is_empty() {
                ContactsKind::None
            } else {
                let lower = path.to_ascii_lowercase();
                if lower.ends_with(".vcf") || lower.ends_with(".vcard") {
                    ContactsKind::Vcf
                } else {
                    ContactsKind::Csv
                }
            };
        }
    }

    fn ui_output_format(&mut self, ui: &mut egui::Ui) {
        combo_enum(
            ui,
            "Output format",
            &mut self.form.output_format,
            &OUTPUT_FORMATS_MAIL,
            PATH_W,
        );
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
                required_labeled_text(
                    ui,
                    "Max fps",
                    &mut self.form.media_max_fps,
                    "e.g. 30",
                    SHORT_W,
                );
                required_labeled_text(
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
        format!("Settings: {}", self.export_ini.path.display())
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
            if self.running {
                ui.spinner();
                ui.label("Running…");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.running && ui.button("Cancel").clicked() {
                    self.cancel();
                }
                if !self.logs.is_empty() {
                    if ui.small_button("Clear").clicked() {
                        self.logs.clear();
                        self.sync_log_text();
                    }
                }
            });
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
            let mode = self.mode;
            match mode {
                AppMode::Log => {
                    ui.set_min_size(ui.available_size());
                    self.ui_log_panel(ui);
                }
                AppMode::ValidateContacts | AppMode::Export | AppMode::Reexport => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_enabled_ui(!self.running, |ui| match mode {
                            AppMode::ValidateContacts => self.ui_validate(ui),
                            AppMode::Export => self.ui_export(ui),
                            AppMode::Reexport => self.ui_reexport(ui),
                            AppMode::Log => unreachable!(),
                        });
                    });
                }
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_export_ini();
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

fn form_label(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) {
    // Fixed LABEL_W column (keeps fields aligned); right-to-left packs label against the inputs.
    let label = label.into();
    ui.allocate_ui_with_layout(
        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.add(egui::Label::new(label).truncate());
        },
    );
}

fn required_field_label(ui: &egui::Ui, text: &str) -> egui::text::LayoutJob {
    let style = ui.style();
    let mut job = egui::text::LayoutJob::default();
    egui::RichText::new(text).append_to(
        &mut job,
        style,
        egui::FontSelection::Default,
        egui::Align::Center,
    );
    egui::RichText::new(" *")
        .small_raised()
        .color(style.visuals.error_fg_color)
        .append_to(
            &mut job,
            style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    job
}

fn required_field_note(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("* Required field")
            .small()
            .color(ui.visuals().weak_text_color()),
    );
}

fn form_action_row(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        form_label(ui, "");
        add(ui);
    });
}

fn form_action_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(label).min_size(egui::vec2(88.0, ui.spacing().interact_size.y)),
    )
}

fn responsive_field_width(ui: &egui::Ui, max_width: f32, trailing_buttons: usize) -> f32 {
    let spacing = ui.spacing().item_spacing.x;
    let trailing_width =
        trailing_buttons as f32 * (PICKER_BUTTON_W + spacing) + LABEL_W + spacing;
    (ui.available_width() - trailing_width)
        .max(MIN_FIELD_W.min(max_width))
        .min(max_width)
}

/// Reserve an exact field width so sibling controls cannot shrink it unexpectedly.
fn with_field_width(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        add,
    );
}

fn labeled_text(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str, width: f32) {
    ui.horizontal(|ui| {
        let width = responsive_field_width(ui, width, 0);
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

fn required_labeled_text(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    width: f32,
) {
    ui.horizontal(|ui| {
        let width = responsive_field_width(ui, width, 0);
        let label = required_field_label(ui, label);
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
    path_or_text_labeled(ui, label, label, value, hint, allow_file, allow_folder);
}

fn required_path_or_text(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    allow_file: bool,
    allow_folder: bool,
) {
    let display_label = required_field_label(ui, label);
    path_or_text_labeled(
        ui,
        display_label,
        label,
        value,
        hint,
        allow_file,
        allow_folder,
    );
}

fn path_or_text_labeled(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    id_salt: &str,
    value: &mut String,
    hint: &str,
    allow_file: bool,
    allow_folder: bool,
) {
    ui.horizontal(|ui| {
        let picker_count = usize::from(allow_file) + usize::from(allow_folder);
        let width = responsive_field_width(ui, PATH_W, picker_count);
        form_label(ui, label);
        let mut response = None;
        with_field_width(ui, width, |ui| {
            response = Some(
                ui.add(
                    egui::TextEdit::singleline(value)
                        .id_salt(id_salt)
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
        if allow_file
            && ui
                .add_sized(
                    [PICKER_BUTTON_W, ui.spacing().interact_size.y],
                    egui::Button::new("File…"),
                )
                .on_hover_text("Choose file")
                .clicked()
        {
            let mut dialog = rfd::FileDialog::new();
            if id_salt.to_ascii_lowercase().contains("contact") {
                dialog = dialog.add_filter("Contacts", &["csv", "vcf", "vcard"]);
            }
            if let Some(path) = dialog.pick_file() {
                *value = path.display().to_string();
            }
        }
        if allow_folder
            && ui
                .add_sized(
                    [PICKER_BUTTON_W, ui.spacing().interact_size.y],
                    egui::Button::new("Folder…"),
                )
                .on_hover_text("Choose folder")
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
        let width = responsive_field_width(ui, width, 0);
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

type LibraryJob = Box<
    dyn FnOnce(
            message_exporters_core::CancelFlag,
            mpsc::Sender<ProcessEvent>,
        ) -> Result<(), String>
        + Send,
>;

/// Build an in-process export job from a validated [`ExporterConfig`].
fn library_job_for_exporter(exporter: Exporter, config: ExporterConfig) -> LibraryJob {
    match exporter {
        Exporter::GoSmsPro => Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_go_sms_pro(&config), tx)
        }),
        Exporter::SmsBackupRestore => Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_sms_restore(&config), tx)
        }),
        Exporter::SmsBackupPlus => Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_sms_plus(&config), tx)
        }),
        Exporter::OpenExtract => Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_openextract(&config), tx)
        }),
        Exporter::Imazing => Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_imazing(&config), tx)
        }),
        Exporter::Whatsapp => Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_whatsapp(&config), tx)
        }),
        Exporter::Imessage => Box::new(move |cancel, tx| {
            let mut config = config;
            config.cancel = Some(cancel);
            run_and_log(run_imessage(&config), tx)
        }),
    }
}

fn run_and_log<R, E: std::fmt::Display>(
    result: Result<R, E>,
    tx: mpsc::Sender<ProcessEvent>,
) -> Result<(), String>
where
    R: HasMessages,
{
    match result {
        Ok(run) => {
            for line in run.into_messages() {
                let _ = tx.send(ProcessEvent::Log(line));
            }
            Ok(())
        }
        Err(error) => Err(format!("{error:#}")),
    }
}

trait HasMessages {
    fn into_messages(self) -> Vec<String>;
}

macro_rules! impl_has_messages {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl HasMessages for $ty {
                fn into_messages(self) -> Vec<String> {
                    self.messages
                }
            }
        )+
    };
}

impl_has_messages!(
    go_sms_pro_exporter::RunResult,
    sms_backup_restore_exporter::RunResult,
    sms_backup_plus_exporter::RunResult,
    openextract_exporter::RunResult,
    imazing_exporter::RunResult,
    imessage_ir_exporter::RunResult,
    whatsapp_exporter::RunResult,
    message_reexporter::RunResult,
);
