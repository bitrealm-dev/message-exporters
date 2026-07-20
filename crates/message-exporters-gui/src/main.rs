mod exporters;
mod process;

use exporters::{
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, ApplePlatform, CONTACT_KINDS, ContactsKind,
    EXPORTERS, Exporter, Form, MAX_RESOLUTIONS, AttachmentMedia, default_output_dir,
};
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_media::{process_near_vault_media, MaxResolution};
use iced::widget::{
    button, checkbox, column, container, pick_list, rich_text, row, rule, scrollable, space, span,
    text, text_input,
};
use iced::{Background, Border, Color, Element, Fill, Shadow, Theme, color};
use iced::{Task, theme};
use process::{ProcessControl, ProcessEvent};

pub fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Message Exporters")
        .theme(App::theme)
        .run()
}

fn flat_theme() -> Theme {
    Theme::custom(
        "Flat",
        theme::Palette {
            background: color!(0xF2F2F2),
            text: color!(0x1E1F22),
            primary: color!(0x3574F0),
            success: color!(0x369A3F),
            danger: color!(0xDB5860),
            warning: color!(0xE2A203),
        },
    )
}

fn panel_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.06),
            offset: iced::Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
        snap: true,
    }
}

fn log_panel_style(theme: &Theme) -> container::Style {
    let mut style = panel_style(theme);
    style.background = Some(Background::Color(color!(0xFAFAFA)));
    style
}

fn error_panel_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.danger.weak.color)),
        text_color: Some(palette.danger.strong.color),
        border: Border {
            color: palette.danger.base.color,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

#[derive(Debug, Clone, Copy)]
enum Field {
    Input,
    Output,
    Contacts,
    OwnerPhones,
    OwnerEmails,
    NameMapping,
    Timezone,
    Seed,
    DbPath,
    AttachmentRoot,
    StartDate,
    EndDate,
    ConversationFilter,
    AppleContacts,
    BackupPassword,
    MediaMaxFps,
    MediaMinSize,
}

#[derive(Debug, Clone, Copy)]
enum PickerKind {
    File,
    Folder,
}

#[derive(Debug, Clone)]
enum Message {
    ExporterSelected(Exporter),
    FieldChanged(Field, String),
    ContactsKindChanged(ContactsKind),
    AttachmentMediaChanged(AttachmentMedia),
    MediaMaxResolutionChanged(MaxResolution),
    ToggleMediaSkipEfficient(bool),
    ApplePlatformChanged(ApplePlatform),
    ToggleAnonymize(bool),
    ToggleAdvanced,
    OpenProductUrl(&'static str),
    Browse(Field, PickerKind),
    Picked(Field, Option<String>),
    Run,
    Cancel,
    ClearLog,
    Process(ProcessEvent),
}

#[derive(Debug)]
struct App {
    exporter: Exporter,
    form: Form,
    running: bool,
    control: ProcessControl,
    logs: Vec<String>,
    errors: Vec<String>,
}

impl Default for App {
    fn default() -> Self {
        let exporter = Exporter::default();
        Self {
            exporter,
            form: Form {
                output: default_output_dir(exporter),
                ..Form::default()
            },
            running: false,
            control: ProcessControl::default(),
            logs: Vec::new(),
            errors: Vec::new(),
        }
    }
}

impl App {
    fn theme(&self) -> Theme {
        flat_theme()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ExporterSelected(exporter) => {
                let previous_default = default_output_dir(self.exporter);
                if self.form.output.trim().is_empty() || self.form.output == previous_default {
                    self.form.output = default_output_dir(exporter);
                }
                self.exporter = exporter;
                self.form.advanced = false;
                self.errors.clear();
            }
            Message::FieldChanged(field, value) => self.set_field(field, value),
            Message::ContactsKindChanged(kind) => self.form.contacts_kind = kind,
            Message::AttachmentMediaChanged(method) => self.form.attachment_media = method,
            Message::MediaMaxResolutionChanged(res) => self.form.media_max_resolution = res,
            Message::ToggleMediaSkipEfficient(value) => self.form.media_skip_efficient = value,
            Message::ApplePlatformChanged(platform) => self.form.apple_platform = platform,
            Message::ToggleAnonymize(value) => self.form.anonymize = value,
            Message::ToggleAdvanced => self.form.advanced = !self.form.advanced,
            Message::OpenProductUrl(url) => {
                if let Err(error) = open::that(url) {
                    self.errors = vec![format!("Could not open link: {error}")];
                }
            }
            Message::Browse(field, kind) => {
                return Task::perform(pick_path(kind), move |path| Message::Picked(field, path));
            }
            Message::Picked(field, Some(path)) => {
                self.set_field(field, path);
            }
            Message::Picked(_, None) => {}
            Message::Run => return self.start_run(),
            Message::Cancel => match self.control.cancel() {
                Ok(()) => self.logs.push("Cancellation requested…".into()),
                Err(error) => self.errors = vec![error],
            },
            Message::ClearLog => self.logs.clear(),
            Message::Process(event) => match event {
                ProcessEvent::Started(command) => {
                    self.logs.push(format!("Running: {command}"));
                }
                ProcessEvent::Log(line) => self.logs.push(line),
                ProcessEvent::Finished(summary) => {
                    self.logs.push(summary);
                    if self.exporter == Exporter::Imessage {
                        self.run_imessage_media_post();
                    }
                    self.running = false;
                }
                ProcessEvent::Error(error) => {
                    self.errors = vec![error.clone()];
                    self.logs.push(format!("Error: {error}"));
                    self.running = false;
                }
            },
        }
        Task::none()
    }

    fn set_field(&mut self, field: Field, value: String) {
        match field {
            Field::Input => self.form.input = value,
            Field::Output => self.form.output = value,
            Field::Contacts => self.form.contacts = value,
            Field::OwnerPhones => self.form.owner_phones = value,
            Field::OwnerEmails => self.form.owner_emails = value,
            Field::NameMapping => self.form.name_mapping = value,
            Field::Timezone => self.form.timezone = value,
            Field::Seed => self.form.anonymize_seed = value,
            Field::DbPath => self.form.db_path = value,
            Field::AttachmentRoot => self.form.attachment_root = value,
            Field::StartDate => self.form.start_date = value,
            Field::EndDate => self.form.end_date = value,
            Field::ConversationFilter => self.form.conversation_filter = value,
            Field::AppleContacts => self.form.apple_contacts = value,
                    Field::BackupPassword => self.form.backup_password = value,
            Field::MediaMaxFps => self.form.media_max_fps = value,
            Field::MediaMinSize => self.form.media_min_size = value,
        }
        self.errors.clear();
    }

    fn start_run(&mut self) -> Task<Message> {
        if self.running {
            return Task::none();
        }
        let args = match self.form.build_args(self.exporter) {
            Ok(args) => args,
            Err(errors) => {
                self.errors = errors;
                return Task::none();
            }
        };
        let program = match process::resolve_binary(self.exporter.binary()) {
            Ok(program) => program,
            Err(error) => {
                self.errors = vec![error];
                return Task::none();
            }
        };
        self.errors.clear();
        self.running = true;
        self.logs.clear();
        Task::run(
            process::run(program, args, self.control.clone()),
            Message::Process,
        )
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

    fn view(&self) -> Element<'_, Message> {
        let header = row![
            column![
                text("Message Exporters").size(26),
                text("Convert phone backups into readable conversation CSV")
                    .size(14)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.weak.text),
                    }),
            ]
            .spacing(4),
            space::horizontal(),
            row![
                text("Backup source").size(14),
                pick_list(EXPORTERS, Some(self.exporter), Message::ExporterSelected).width(220),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        ]
        .align_y(iced::Alignment::Center);

        let global = container(self.global_options_view())
            .padding(18)
            .width(Fill)
            .style(panel_style);
        let form = container(self.form_view())
            .padding(18)
            .width(Fill)
            .style(panel_style);
        let logs = self.log_view();
        let content = column![header, global, form, logs]
            .spacing(16)
            .padding(20)
            .width(Fill);
        container(scrollable(content))
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn global_options_view(&self) -> Element<'_, Message> {
        column![
            text("Global options").size(18),
            text("Applied to every backup source").size(13),
            self.anonymize_view(),
            self.date_range_view(),
        ]
        .spacing(12)
        .into()
    }

    fn form_view(&self) -> Element<'_, Message> {
        let title = rich_text([span(self.exporter.link_label())
            .link(self.exporter.product_url())
            .color(color!(0x3574F0))
            .underline(true)])
        .size(20)
        .on_link_click(Message::OpenProductUrl);

        let mut fields = column![title].spacing(14);

        if self.exporter == Exporter::Imessage {
            fields = fields.push(path_row(
                "Database / iOS backup path",
                &self.form.db_path,
                Field::DbPath,
                true,
                true,
            ));
            fields = fields.push(password_row(&self.form.backup_password));
            fields = fields.push(
                row![
                    text("Platform").width(200),
                    pick_list(
                        APPLE_PLATFORMS,
                        Some(self.form.apple_platform),
                        Message::ApplePlatformChanged
                    )
                    .width(Fill)
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            );
            fields = fields.push(path_row(
                "Output directory",
                &self.form.output,
                Field::Output,
                false,
                true,
            ));
            fields = fields.push(self.attachment_media_view());
            fields = fields.push(self.advanced_toggle());
            if self.form.advanced {
                fields = fields.push(self.imessage_advanced());
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
            fields = fields.push(path_row(
                input_label,
                &self.form.input,
                Field::Input,
                file,
                folder,
            ));
            fields = fields.push(path_row(
                "Output directory",
                &self.form.output,
                Field::Output,
                false,
                true,
            ));

            if matches!(
                self.exporter,
                Exporter::GoSmsPro | Exporter::SmsBackupRestore | Exporter::SmsBackupPlus
            ) {
                fields = fields.push(input_row(
                    "Your phone number(s)",
                    "Comma-separated phone numbers",
                    &self.form.owner_phones,
                    Field::OwnerPhones,
                ));
                fields = fields.push(self.attachment_media_view());
            }
            if self.exporter == Exporter::SmsBackupPlus {
                fields = fields.push(input_row(
                    "Your email address(es)",
                    "Comma-separated email addresses",
                    &self.form.owner_emails,
                    Field::OwnerEmails,
                ));
            }

            fields = fields.push(self.contacts_view());
            if self.exporter == Exporter::Imazing {
                fields = fields.push(input_row(
                    "Timezone",
                    "IANA name, e.g. America/New_York (blank = local)",
                    &self.form.timezone,
                    Field::Timezone,
                ));
            }

            if self.exporter == Exporter::SmsBackupPlus {
                fields = fields.push(self.advanced_toggle());
                if self.form.advanced {
                    fields = fields.push(path_row(
                        "Name mapping CSV",
                        &self.form.name_mapping,
                        Field::NameMapping,
                        true,
                        false,
                    ));
                }
            }
        }

        if !self.errors.is_empty() {
            fields = fields.push(
                container(
                    column(
                        self.errors
                            .iter()
                            .map(|error| text(format!("• {error}")).into())
                            .collect::<Vec<Element<'_, Message>>>(),
                    )
                    .spacing(4),
                )
                .padding(12)
                .width(Fill)
                .style(error_panel_style),
            );
        }

        let run_button = if self.running {
            button("Running…").style(button::primary)
        } else {
            button("Run exporter")
                .style(button::primary)
                .on_press(Message::Run)
        };
        let cancel_button = if self.running {
            button("Cancel")
                .style(button::secondary)
                .on_press(Message::Cancel)
        } else {
            button("Cancel").style(button::secondary)
        };
        fields
            .push(row![run_button, cancel_button].spacing(10))
            .into()
    }

    fn attachment_media_view(&self) -> Element<'_, Message> {
        let mut content = column![
            row![
                text("Attachments").width(200),
                pick_list(
                    ATTACHMENT_MEDIA,
                    Some(self.form.attachment_media),
                    Message::AttachmentMediaChanged
                )
                .width(Fill),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(8);

        if self.form.attachment_media.needs_ffmpeg() && !message_media::ffmpeg_available() {
            content = content.push(
                text("Convert/Compress need ffmpeg and ffprobe on PATH.")
                    .size(13)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().danger.base.color),
                    }),
            );
        }

        if self.form.attachment_media == AttachmentMedia::Compress {
            let nested = column![
                row![
                    text("Max resolution").width(200),
                    pick_list(
                        MAX_RESOLUTIONS,
                        Some(self.form.media_max_resolution),
                        Message::MediaMaxResolutionChanged
                    )
                    .width(Fill),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                input_row(
                    "Max fps",
                    "e.g. 30",
                    &self.form.media_max_fps,
                    Field::MediaMaxFps,
                ),
                input_row(
                    "Min size",
                    "e.g. 20M",
                    &self.form.media_min_size,
                    Field::MediaMinSize,
                ),
                row![
                    text("Skip efficient").width(200),
                    checkbox(self.form.media_skip_efficient)
                        .label("Skip already-efficient HEVC")
                        .on_toggle(Message::ToggleMediaSkipEfficient),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            ]
            .spacing(8);
            content = content.push(container(nested).padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 24.0,
            }));
        }

        content.into()
    }

    fn advanced_toggle(&self) -> Element<'_, Message> {
        let chevron = if self.form.advanced { "▾" } else { "▸" };
        button(text(format!("{chevron}  Show advanced options")).size(14))
            .style(button::text)
            .padding([4, 0])
            .on_press(Message::ToggleAdvanced)
            .into()
    }

    fn contacts_view(&self) -> Element<'_, Message> {
        if self.exporter == Exporter::Imazing {
            return path_row(
                "iMazing Contacts CSV (recommended)",
                &self.form.contacts,
                Field::Contacts,
                true,
                false,
            );
        }
        column![
            row![
                text("Contacts").width(200),
                pick_list(
                    CONTACT_KINDS,
                    Some(self.form.contacts_kind),
                    Message::ContactsKindChanged
                )
                .width(Fill),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            if self.form.contacts_kind == ContactsKind::None {
                text("No contacts: phone numbers may not resolve to names.")
                    .size(13)
                    .into()
            } else {
                path_row(
                    match self.form.contacts_kind {
                        ContactsKind::Csv => "Contacts CSV",
                        ContactsKind::Vcf => "Contacts VCF",
                        ContactsKind::None => "",
                    },
                    &self.form.contacts,
                    Field::Contacts,
                    true,
                    false,
                )
            }
        ]
        .spacing(8)
        .into()
    }

    fn anonymize_view(&self) -> Element<'_, Message> {
        let mut content = column![
            checkbox(self.form.anonymize)
                .label("Anonymize")
                .on_toggle(Message::ToggleAnonymize)
        ]
        .spacing(8);
        if self.form.anonymize || !self.form.anonymize_seed.is_empty() {
            content = content.push(seed_row(&self.form.anonymize_seed));
        }
        content.into()
    }

    fn date_range_view(&self) -> Element<'_, Message> {
        column![
            input_row(
                "Start date",
                "YYYY-MM-DD (inclusive, blank = no limit)",
                &self.form.start_date,
                Field::StartDate
            ),
            input_row(
                "End date",
                "YYYY-MM-DD (exclusive, blank = no limit)",
                &self.form.end_date,
                Field::EndDate
            ),
        ]
        .spacing(12)
        .into()
    }

    fn imessage_advanced(&self) -> Element<'_, Message> {
        column![
            path_row(
                "Attachment root",
                &self.form.attachment_root,
                Field::AttachmentRoot,
                false,
                true,
            ),
            input_row(
                "Conversation filter",
                "Names, numbers, or emails",
                &self.form.conversation_filter,
                Field::ConversationFilter
            ),
            path_row(
                "Apple AddressBook DB",
                &self.form.apple_contacts,
                Field::AppleContacts,
                true,
                false,
            ),
        ]
        .spacing(12)
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 8.0,
        })
        .into()
    }

    fn log_view(&self) -> Element<'_, Message> {
        let actions = row![
            text("Run log").size(16),
            space::horizontal(),
            button("Clear")
                .style(button::text)
                .on_press(Message::ClearLog)
        ]
        .align_y(iced::Alignment::Center);
        let body = if self.logs.is_empty() {
            column![text("Exporter output will appear here.").size(13)]
        } else {
            column(
                self.logs
                    .iter()
                    .map(|line| text(line).size(13).into())
                    .collect::<Vec<Element<'_, Message>>>(),
            )
            .spacing(3)
        };
        container(
            column![
                actions,
                rule::horizontal(1),
                scrollable(body).height(220)
            ]
            .spacing(10),
        )
        .padding(14)
        .width(Fill)
        .style(log_panel_style)
        .into()
    }
}

fn input_row<'a>(
    label: &'a str,
    placeholder: &'a str,
    value: &'a str,
    field: Field,
) -> Element<'a, Message> {
    row![
        text(label).width(200),
        text_input(placeholder, value)
            .on_input(move |value| Message::FieldChanged(field, value))
            .width(Fill)
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into()
}

fn path_row<'a>(
    label: &'a str,
    value: &'a str,
    field: Field,
    allow_file: bool,
    allow_folder: bool,
) -> Element<'a, Message> {
    let mut controls = row![
        text(label).width(200),
        text_input("Path", value)
            .on_input(move |value| Message::FieldChanged(field, value))
            .width(Fill)
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if allow_file {
        controls = controls.push(
            button("File…")
                .style(button::secondary)
                .on_press(Message::Browse(field, PickerKind::File)),
        );
    }
    if allow_folder {
        controls = controls.push(
            button("Folder…")
                .style(button::secondary)
                .on_press(Message::Browse(field, PickerKind::Folder)),
        );
    }
    controls.into()
}

fn seed_row(value: &str) -> Element<'_, Message> {
    input_row(
        "Seed",
        "Optional 64-character hex seed",
        value,
        Field::Seed,
    )
}

fn password_row(value: &str) -> Element<'_, Message> {
    row![
        text("Backup password").width(200),
        text_input("Encrypted iOS backup password", value)
            .on_input(|value| Message::FieldChanged(Field::BackupPassword, value))
            .secure(true)
            .width(Fill)
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .into()
}

async fn pick_path(kind: PickerKind) -> Option<String> {
    let dialog = rfd::AsyncFileDialog::new().set_title("Choose path");
    match kind {
        PickerKind::File => dialog
            .pick_file()
            .await
            .map(|file| file.path().display().to_string()),
        PickerKind::Folder => dialog
            .pick_folder()
            .await
            .map(|folder| folder.path().display().to_string()),
    }
}
