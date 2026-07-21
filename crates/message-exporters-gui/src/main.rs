//! egui front-end for message-exporters (Validate contacts + Export).

mod exporters;
mod process;

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui;
use exporters::{
    default_output_dir, AttachmentMedia, ContactsKind, Exporter, Form, APPLE_PLATFORMS,
    ATTACHMENT_MEDIA, CONTACT_KINDS, EXPORTERS, MAX_RESOLUTIONS,
};
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_media::process_near_vault_media;
use process::{ProcessControl, ProcessEvent};

const LABEL_W: f32 = 130.0;
const PATH_W: f32 = 280.0;
const COMBO_W: f32 = 200.0;
const SHORT_W: f32 = 140.0;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 700.0])
            .with_min_inner_size([560.0, 480.0])
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
    validate_input: String,
    validate_usa: bool,
    running: bool,
    control: ProcessControl,
    logs: Vec<String>,
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
                output: default_output_dir(exporter),
                ..Form::default()
            },
            validate_input: String::new(),
            validate_usa: true,
            running: false,
            control: ProcessControl::default(),
            logs: Vec::new(),
            errors: Vec::new(),
            rx: None,
        }
    }
}

impl App {
    fn poll_events(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else {
            return;
        };
        while let Ok(event) = rx.try_recv() {
            match event {
                ProcessEvent::Started(command) => {
                    self.logs.push(format!("Running: {command}"));
                }
                ProcessEvent::Log(line) => self.logs.push(line),
                ProcessEvent::Finished(summary) => {
                    self.logs.push(summary);
                    if self.mode == AppMode::Export && self.exporter == Exporter::Imessage {
                        self.run_imessage_media_post();
                    }
                    self.running = false;
                    self.rx = None;
                    break;
                }
                ProcessEvent::Error(error) => {
                    self.errors = vec![error.clone()];
                    self.logs.push(format!("Error: {error}"));
                    self.running = false;
                    self.rx = None;
                    break;
                }
            }
        }
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn start_export(&mut self) {
        if self.running {
            return;
        }
        let args = match self.form.build_args(self.exporter) {
            Ok(args) => args,
            Err(errors) => {
                self.errors = errors;
                return;
            }
        };
        let program = match process::resolve_binary(self.exporter.binary()) {
            Ok(program) => program,
            Err(error) => {
                self.errors = vec![error];
                return;
            }
        };
        self.errors.clear();
        self.running = true;
        self.logs.clear();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        process::spawn(program, args, self.control.clone(), tx);
    }

    fn start_validate(&mut self, check_only: bool) {
        if self.running {
            return;
        }
        let mut errors = Vec::new();
        let input = self.validate_input.trim();
        if input.is_empty() {
            errors.push("Choose a contacts CSV or VCF file.".into());
        } else if !std::path::Path::new(input).is_file() {
            errors.push(format!("Contacts file not found: {input}"));
        }
        if !errors.is_empty() {
            self.errors = errors;
            return;
        }

        let program = match process::resolve_binary("contacts-validate") {
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
        self.logs.clear();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        process::spawn(program, args, self.control.clone(), tx);
    }

    fn cancel(&mut self) {
        match self.control.cancel() {
            Ok(()) => self.logs.push("Cancellation requested…".into()),
            Err(error) => self.errors = vec![error],
        }
    }

    fn run_imessage_media_post(&mut self) {
        let mode = self.form.attachment_media.media_mode();
        if matches!(mode, message_media::MediaMode::Disabled) {
            return;
        }
        let output = std::path::PathBuf::from(self.form.output.trim());
        if mode.needs_tools() {
            self.logs
                .push(format!("Processing attachment media ({mode})…"));
            let compress = match self.form.compress_options() {
                Ok(opts) => opts,
                Err(error) => {
                    self.errors = vec![error.clone()];
                    self.logs.push(format!("Error: {error}"));
                    return;
                }
            };
            match process_near_vault_media(&output, mode, &compress) {
                Ok(report) => {
                    if report.processed > 0 || report.skipped > 0 || !report.errors.is_empty() {
                        self.logs.push(format!(
                            "Media: processed {} file(s), skipped {}, updated {} CSV(s)",
                            report.processed, report.skipped, report.csv_files_updated
                        ));
                    }
                    for err in report.errors.iter().take(10) {
                        self.logs.push(format!("media warning: {err}"));
                    }
                }
                Err(error) => {
                    let msg = format!("Media processing failed: {error}");
                    self.errors = vec![msg.clone()];
                    self.logs.push(msg);
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
                Ok((n, _)) => self
                    .logs
                    .push(format!("Anonymized {n} CSV file(s) under {}", output.display())),
                Err(error) => {
                    let msg = format!("Anonymize failed: {error}");
                    self.errors = vec![msg.clone()];
                    self.logs.push(msg);
                }
            }
        }
    }

    fn ui_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.running, |ui| {
                ui.selectable_value(
                    &mut self.mode,
                    AppMode::ValidateContacts,
                    "Validate contacts",
                );
                ui.selectable_value(&mut self.mode, AppMode::Export, "Export");
            });
        });
    }

    fn ui_validate(&mut self, ui: &mut egui::Ui) {
        ui.heading("Validate contacts");
        ui.label(
            egui::RichText::new(
                "Check analyzes phones without writing files. Update writes \
                 <name>-corrected-<YYMMDD-hhmmss>.<ext> beside the original \
                 (plus .log; CSV also writes .vcf). Uncertain values stay as-is.",
            )
            .weak(),
        );
        ui.add_space(10.0);

        path_or_text(
            ui,
            "Contacts file",
            &mut self.validate_input,
            "contacts.vcf or contacts.csv",
            true,
            false,
        );

        ui.add_space(6.0);
        ui.checkbox(&mut self.validate_usa, "USA numbers");
        ui.label(
            egui::RichText::new(if self.validate_usa {
                "Certain: 10-digit or 11-digit (leading 1) US numbers → +1…"
            } else {
                "International: only numbers that already start with + are rewritten."
            })
            .small()
            .weak(),
        );

        self.ui_errors(ui);

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let check = ui.add_enabled(!self.running, egui::Button::new("Check"));
            if check.clicked() {
                self.start_validate(true);
            }
            let update = ui.add_enabled(!self.running, egui::Button::new("Update"));
            if update.clicked() {
                self.start_validate(false);
            }
            let cancel = ui.add_enabled(self.running, egui::Button::new("Cancel"));
            if cancel.clicked() {
                self.cancel();
            }
        });
    }

    fn ui_export(&mut self, ui: &mut egui::Ui) {
        ui.heading("Export");
        ui.label(
            egui::RichText::new("Convert phone backups into readable conversation CSV").weak(),
        );
        ui.add_space(8.0);

        ui.heading(egui::RichText::new("Global options").size(16.0));
        ui.checkbox(&mut self.form.anonymize, "Anonymize");
        if self.form.anonymize || !self.form.anonymize_seed.is_empty() {
            labeled_text(
                ui,
                "Seed",
                &mut self.form.anonymize_seed,
                "Optional 64-hex seed",
                PATH_W,
            );
        }
        labeled_text(
            ui,
            "Start date",
            &mut self.form.start_date,
            "YYYY-MM-DD",
            SHORT_W,
        );
        labeled_text(
            ui,
            "End date",
            &mut self.form.end_date,
            "YYYY-MM-DD",
            SHORT_W,
        );

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);

        self.ui_backup_source(ui);
        ui.add_space(6.0);
        if ui
            .link(self.exporter.link_label())
            .on_hover_text(self.exporter.product_url())
            .clicked()
        {
            if let Err(error) = open::that(self.exporter.product_url()) {
                self.errors = vec![format!("Could not open link: {error}")];
            }
        }
        ui.add_space(8.0);

        if self.exporter == Exporter::Imessage {
            path_or_text(
                ui,
                "Database / iOS backup path",
                &mut self.form.db_path,
                "Path",
                true,
                true,
            );
            ui.horizontal(|ui| {
                form_label(ui, "Backup password");
                ui.add(
                    egui::TextEdit::singleline(&mut self.form.backup_password)
                        .password(true)
                        .desired_width(PATH_W)
                        .clip_text(true)
                        .hint_text("Encrypted iOS backup password"),
                );
            });
            combo_enum(
                ui,
                "Platform",
                &mut self.form.apple_platform,
                &APPLE_PLATFORMS,
            );
            path_or_text(
                ui,
                "Output directory",
                &mut self.form.output,
                "Path",
                false,
                true,
            );
            self.ui_attachment_media(ui);
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
            if self.form.advanced {
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
                    "Names, numbers, or emails",
                    false,
                    false,
                );
                path_or_text(
                    ui,
                    "Apple AddressBook DB",
                    &mut self.form.apple_contacts,
                    "Path",
                    true,
                    false,
                );
            }
        } else {
            let (file, folder) = match self.exporter {
                Exporter::GoSmsPro => (false, true),
                Exporter::SmsBackupRestore
                | Exporter::SmsBackupPlus
                | Exporter::OpenExtract
                | Exporter::Imazing => (true, true),
                Exporter::Imessage => unreachable!(),
            };
            let input_label = if self.exporter == Exporter::SmsBackupPlus {
                "Input file or folder"
            } else {
                "Input source"
            };
            path_or_text(ui, input_label, &mut self.form.input, "Path", file, folder);
            path_or_text(
                ui,
                "Output directory",
                &mut self.form.output,
                "Path",
                false,
                true,
            );

            if matches!(
                self.exporter,
                Exporter::GoSmsPro | Exporter::SmsBackupRestore | Exporter::SmsBackupPlus
            ) {
                path_or_text(
                    ui,
                    "Your phone number(s)",
                    &mut self.form.owner_phones,
                    "Comma-separated phone numbers",
                    false,
                    false,
                );
                self.ui_attachment_media(ui);
            }
            if self.exporter == Exporter::SmsBackupPlus {
                path_or_text(
                    ui,
                    "Your email address(es)",
                    &mut self.form.owner_emails,
                    "Comma-separated email addresses",
                    false,
                    false,
                );
            }

            self.ui_contacts(ui);

            if self.exporter == Exporter::Imazing {
                path_or_text(
                    ui,
                    "Timezone",
                    &mut self.form.timezone,
                    "IANA name, e.g. America/New_York",
                    false,
                    false,
                );
            }

            if self.exporter == Exporter::SmsBackupPlus {
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
                if self.form.advanced {
                    path_or_text(
                        ui,
                        "Name mapping CSV",
                        &mut self.form.name_mapping,
                        "Path",
                        true,
                        false,
                    );
                }
            }
        }

        self.ui_errors(ui);

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let run = ui.add_enabled(!self.running, egui::Button::new("Run exporter"));
            if run.clicked() {
                self.start_export();
            }
            let cancel = ui.add_enabled(self.running, egui::Button::new("Cancel"));
            if cancel.clicked() {
                self.cancel();
            }
        });
    }

    fn ui_backup_source(&mut self, ui: &mut egui::Ui) {
        let previous = self.exporter;
        ui.horizontal(|ui| {
            form_label(ui, "Backup source");
            egui::ComboBox::from_id_salt("exporter")
                .selected_text(self.exporter.display_name())
                .width(COMBO_W)
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
        if self.exporter != previous {
            let previous_default = default_output_dir(previous);
            if self.form.output.trim().is_empty() || self.form.output == previous_default {
                self.form.output = default_output_dir(self.exporter);
            }
            self.form.advanced = false;
            self.errors.clear();
        }
    }

    fn ui_contacts(&mut self, ui: &mut egui::Ui) {
        if self.exporter == Exporter::Imazing {
            path_or_text(
                ui,
                "iMazing Contacts CSV (recommended)",
                &mut self.form.contacts,
                "Path",
                true,
                false,
            );
            return;
        }
        combo_enum(
            ui,
            "Contacts",
            &mut self.form.contacts_kind,
            &CONTACT_KINDS,
        );
        if self.form.contacts_kind == ContactsKind::None {
            ui.label(
                egui::RichText::new("No contacts: phone numbers may not resolve to names.")
                    .small()
                    .weak(),
            );
        } else {
            let label = match self.form.contacts_kind {
                ContactsKind::Csv => "Contacts CSV",
                ContactsKind::Vcf => "Contacts VCF",
                ContactsKind::None => "",
            };
            path_or_text(ui, label, &mut self.form.contacts, "Path", true, false);
        }
    }

    fn ui_attachment_media(&mut self, ui: &mut egui::Ui) {
        combo_enum(
            ui,
            "Attachments",
            &mut self.form.attachment_media,
            &ATTACHMENT_MEDIA,
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

    fn ui_log(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Run log").size(16.0));
            if ui.button("Clear").clicked() {
                self.logs.clear();
            }
        });
        let empty = match self.mode {
            AppMode::Export => "Exporter output will appear here.",
            AppMode::ValidateContacts => "Validation output will appear here.",
        };
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(220.0)
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                if self.logs.is_empty() {
                    ui.label(egui::RichText::new(empty).weak().monospace());
                } else {
                    for line in &self.logs {
                        ui.add(
                            egui::Label::new(egui::RichText::new(line).monospace())
                                .wrap()
                                .truncate(),
                        )
                        .on_hover_text(line);
                    }
                }
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events(ctx);

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            self.ui_tabs(ui);
            ui.add_space(2.0);
        });

        egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(240.0)
            .show(ctx, |ui| {
                self.ui_log(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.mode {
                    AppMode::ValidateContacts => self.ui_validate(ui),
                    AppMode::Export => self.ui_export(ui),
                }
            });
        });
    }
}

fn form_label(ui: &mut egui::Ui, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(LABEL_W, ui.spacing().interact_size.y),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(label);
        },
    );
}

fn labeled_text(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str, width: f32) {
    ui.horizontal(|ui| {
        form_label(ui, label);
        let response = ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(width)
                .clip_text(true)
                .hint_text(hint),
        );
        if !value.is_empty() {
            response.on_hover_text(value.as_str());
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
        let response = ui.add(
            egui::TextEdit::singleline(value)
                .id_salt(label)
                .desired_width(PATH_W)
                .clip_text(true)
                .hint_text(hint),
        );
        if !value.is_empty() {
            response.on_hover_text(value.as_str());
        }
        if allow_file && ui.button("File…").clicked() {
            let mut dialog = rfd::FileDialog::new();
            if label.to_ascii_lowercase().contains("contact") {
                dialog = dialog.add_filter("Contacts", &["csv", "vcf", "vcard"]);
            }
            if let Some(path) = dialog.pick_file() {
                *value = path.display().to_string();
            }
        }
        if allow_folder && ui.button("Folder…").clicked() {
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
) {
    ui.horizontal(|ui| {
        form_label(ui, label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(value.to_string())
            .width(COMBO_W)
            .show_ui(ui, |ui| {
                for opt in options {
                    ui.selectable_value(value, *opt, opt.to_string());
                }
            });
    });
}
