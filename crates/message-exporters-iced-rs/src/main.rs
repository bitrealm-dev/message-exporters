//! iced front-end for message-exporters (Contacts validate + Message export).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use chrono::Local;
use iced::widget::{
    button, checkbox, column, container, pick_list, radio, row, rule, scrollable, space, svg, text,
    text_editor, text_input, Column, Space,
};
use iced::widget::scrollable::{Direction as ScrollDirection, Scrollbar};
use iced::widget::svg::Handle as SvgHandle;
use iced::{Alignment, Element, Fill, Font, Length, Subscription, Task};

const LOG_PLACEHOLDER: &str = "Output of the current operation will appear here.";
/// Extra window height added when the log pane opens so it grows downward.
const LOG_PANE_HEIGHT: f32 = 200.0;
const WINDOW_MIN_HEIGHT: f32 = 360.0;
const UI_TITLE_SIZE: f32 = 16.0;
const UI_BODY_SIZE: f32 = 13.0;
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_csv::DateRange;
use message_exporters_core::{
    default_output_dir, ensure_output_dir, resolve_binary, spawn, ApplePlatform, AttachmentMedia,
    ContactsKind, Exporter, Form, ProcessControl, ProcessEvent, APPLE_PLATFORMS, ATTACHMENT_MEDIA,
    EXPORTERS, MAX_RESOLUTIONS,
};
use message_media::{process_near_vault_media, MaxResolution, MediaMode};

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Message Exporters")
        .window(iced::window::Settings {
            size: iced::Size::new(560.0, 420.0),
            min_size: Some(iced::Size::new(480.0, 360.0)),
            ..Default::default()
        })
        .subscription(App::subscription)
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AppMode {
    #[default]
    Contacts,
    Message,
}

struct App {
    mode: AppMode,
    exporter: Exporter,
    form: Form,
    validate_input: String,
    validate_usa: bool,
    /// Path last successfully Updated; Update stays disabled until the path changes.
    last_updated_input: Option<String>,
    /// True while a Contacts Update (not Check) process is running.
    pending_validate_update: bool,
    log_expanded: bool,
    /// Window height before the log pane opened; restored on roll-up.
    window_height_before_log: Option<f32>,
    /// Basename shown in the log chevron (no directory).
    session_log_name: Option<String>,
    session_log_path: Option<PathBuf>,
    running: bool,
    control: ProcessControl,
    logs: Vec<String>,
    log_content: text_editor::Content,
    errors: Vec<String>,
    rx: Option<Receiver<ProcessEvent>>,
}

impl Default for App {
    fn default() -> Self {
        let exporter = Exporter::default();
        Self {
            mode: AppMode::Contacts,
            exporter,
            form: Form {
                output: default_output_dir(exporter, ""),
                ..Form::default()
            },
            validate_input: String::new(),
            validate_usa: true,
            last_updated_input: None,
            pending_validate_update: false,
            log_expanded: false,
            window_height_before_log: None,
            session_log_name: None,
            session_log_path: None,
            running: false,
            control: ProcessControl::default(),
            logs: Vec::new(),
            log_content: text_editor::Content::with_text(LOG_PLACEHOLDER),
            errors: Vec::new(),
            rx: None,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Tab(AppMode),
    Tick,
    ToggleLog,
    /// Captured window size before applying the log expand boost.
    LogWindowBoost(iced::Size),
    LogAction(text_editor::Action),
    // Contacts
    ValidatePath(String),
    ValidateUsa(bool),
    PickValidateFile,
    Check,
    Update,
    Cancel,
    // Export global
    Anonymize(bool),
    Seed(String),
    StartDate(String),
    EndDate(String),
    ExporterSelected(Exporter),
    OpenProductUrl,
    ToggleAdvanced,
    // Paths / fields
    Input(String),
    Output(String),
    OwnerPhones(String),
    OwnerEmails(String),
    Contacts(String),
    Timezone(String),
    NameMapping(String),
    DbPath(String),
    BackupPassword(String),
    ApplePlatform(ApplePlatform),
    AttachmentRoot(String),
    ConversationFilter(String),
    AppleContacts(String),
    AttachmentMedia(AttachmentMedia),
    MaxResolution(MaxResolution),
    MaxFps(String),
    MinSize(String),
    SkipEfficient(bool),
    PickInputFile,
    PickInputFolder,
    PickOutputFolder,
    PickContactsFile,
    PickNameMapping,
    PickDbPath,
    PickDbFolder,
    PickAttachmentRoot,
    PickAppleContacts,
    RunExport,
    /// Idle: clear export form. Running: cancel the export process.
    ClearExport,
}

impl App {
    fn subscription(&self) -> Subscription<Message> {
        if self.running {
            iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tab(mode) => {
                if !self.running {
                    self.mode = mode;
                }
            }
            Message::Tick => self.poll_events(),
            Message::ToggleLog => {
                return self.set_log_expanded(!self.log_expanded);
            }
            Message::LogWindowBoost(size) => {
                if self.window_height_before_log.is_none() {
                    self.window_height_before_log = Some(size.height);
                }
                let target = iced::Size::new(size.width, size.height + LOG_PANE_HEIGHT);
                return iced::window::latest()
                    .and_then(move |id| iced::window::resize(id, target));
            }
            Message::LogAction(action) => {
                // Selection + scroll (+ Ctrl+C); block typing/paste into the log.
                if !matches!(action, text_editor::Action::Edit(_)) {
                    self.log_content.perform(action);
                }
            }
            Message::ValidatePath(v) => self.set_validate_input(v),
            Message::ValidateUsa(v) => self.validate_usa = v,
            Message::PickValidateFile => {
                if let Some(path) = pick_file(Some(&["csv", "vcf", "vcard"])) {
                    self.set_validate_input(path);
                }
            }
            Message::Check => return self.start_validate(true),
            Message::Update => return self.start_validate(false),
            Message::Cancel => self.cancel(),
            Message::ClearExport => {
                if self.running {
                    self.cancel();
                } else {
                    return self.clear_export_form();
                }
            }
            Message::Anonymize(v) => self.form.anonymize = v,
            Message::Seed(v) => self.form.anonymize_seed = v,
            Message::StartDate(v) => self.form.start_date = v,
            Message::EndDate(v) => self.form.end_date = v,
            Message::ExporterSelected(exporter) => {
                let previous = self.exporter;
                let previous_input = export_input_path(previous, &self.form).to_string();
                self.exporter = exporter;
                let previous_default = default_output_dir(previous, &previous_input);
                if self.form.output.trim().is_empty() || self.form.output == previous_default {
                    self.form.output =
                        default_output_dir(exporter, export_input_path(exporter, &self.form));
                }
                self.form.advanced = false;
                self.errors.clear();
            }
            Message::OpenProductUrl => {
                if let Err(error) = open::that(self.exporter.product_url()) {
                    self.errors = vec![format!("Could not open link: {error}")];
                }
            }
            Message::ToggleAdvanced => self.form.advanced = !self.form.advanced,
            Message::Input(v) => {
                let old = self.form.input.clone();
                self.form.input = v;
                self.sync_default_output(&old, &self.form.input.clone());
            }
            Message::Output(v) => self.form.output = v,
            Message::OwnerPhones(v) => self.form.owner_phones = v,
            Message::OwnerEmails(v) => self.form.owner_emails = v,
            Message::Contacts(v) => return self.set_contacts_path(v),
            Message::Timezone(v) => self.form.timezone = v,
            Message::NameMapping(v) => self.form.name_mapping = v,
            Message::DbPath(v) => {
                let old = self.form.db_path.clone();
                self.form.db_path = v;
                if self.exporter == Exporter::Imessage {
                    self.sync_default_output(&old, &self.form.db_path.clone());
                }
            }
            Message::BackupPassword(v) => self.form.backup_password = v,
            Message::ApplePlatform(v) => self.form.apple_platform = v,
            Message::AttachmentRoot(v) => self.form.attachment_root = v,
            Message::ConversationFilter(v) => self.form.conversation_filter = v,
            Message::AppleContacts(v) => self.form.apple_contacts = v,
            Message::AttachmentMedia(v) => self.form.attachment_media = v,
            Message::MaxResolution(v) => self.form.media_max_resolution = v,
            Message::MaxFps(v) => self.form.media_max_fps = v,
            Message::MinSize(v) => self.form.media_min_size = v,
            Message::SkipEfficient(v) => self.form.media_skip_efficient = v,
            Message::PickInputFile => {
                if let Some(path) = pick_file(None) {
                    let old = self.form.input.clone();
                    self.form.input = path;
                    self.sync_default_output(&old, &self.form.input.clone());
                }
            }
            Message::PickInputFolder | Message::PickOutputFolder | Message::PickDbFolder
            | Message::PickAttachmentRoot => {
                if let Some(path) = pick_folder() {
                    match message {
                        Message::PickInputFolder => {
                            let old = self.form.input.clone();
                            self.form.input = path;
                            self.sync_default_output(&old, &self.form.input.clone());
                        }
                        Message::PickOutputFolder => self.form.output = path,
                        Message::PickDbFolder => {
                            let old = self.form.db_path.clone();
                            self.form.db_path = path;
                            if self.exporter == Exporter::Imessage {
                                self.sync_default_output(&old, &self.form.db_path.clone());
                            }
                        }
                        Message::PickAttachmentRoot => self.form.attachment_root = path,
                        _ => {}
                    }
                }
            }
            Message::PickContactsFile => {
                if let Some(path) = pick_file(Some(&["csv", "vcf", "vcard"])) {
                    return self.set_contacts_path(path);
                }
            }
            Message::PickNameMapping | Message::PickAppleContacts | Message::PickDbPath => {
                let filter = matches!(message, Message::PickNameMapping)
                    .then_some(["csv"].as_slice());
                if let Some(path) = pick_file(filter) {
                    match message {
                        Message::PickNameMapping => self.form.name_mapping = path,
                        Message::PickAppleContacts => self.form.apple_contacts = path,
                        Message::PickDbPath => {
                            let old = self.form.db_path.clone();
                            self.form.db_path = path;
                            if self.exporter == Exporter::Imessage {
                                self.sync_default_output(&old, &self.form.db_path.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::RunExport => return self.start_export(),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let tabs = row![
            tab_button(
                phone_numbers_icon_handle(),
                "Phone Numbers",
                self.mode == AppMode::Contacts,
                Message::Tab(AppMode::Contacts),
            ),
            tab_button(
                message_export_icon_handle(),
                "Message Export",
                self.mode == AppMode::Message,
                Message::Tab(AppMode::Message),
            ),
        ]
        .spacing(8);

        // Form keeps Fill above the log divider; log uses Fixed(LOG_PANE_HEIGHT) so
        // the window boost on expand does not shrink the form viewport.
        // Message Export pins its own title outside an inner scrollable.
        let content: Element<_> = match self.mode {
            AppMode::Contacts => self.view_contacts(),
            AppMode::Message => self.view_export(),
        };

        let mut body = column![tabs, content]
            .spacing(12)
            .padding(18)
            .height(Fill);

        if !self.errors.is_empty() {
            let err_text = self
                .errors
                .iter()
                .map(|e| format!("• {e}"))
                .collect::<Vec<_>>()
                .join("\n");
            body = body.push(
                container(text(err_text).color(iced::Color::from_rgb8(140, 40, 40)))
                    .padding(10)
                    .style(|_theme| container::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb8(
                            255, 235, 235,
                        ))),
                        border: iced::Border {
                            color: iced::Color::from_rgb8(200, 80, 80),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }

        // Fixed boundary: form (Check/Format) above; log chevron + pane below.
        body = body.push(rule::horizontal(1));

        let name = self.session_log_name.as_deref().unwrap_or("(log)");
        let log_label = if self.log_expanded {
            format!("▾ {name}")
        } else {
            format!("▸ {name}")
        };
        body = body.push(
            button(text(log_label).size(UI_BODY_SIZE))
                .style(button::text)
                .on_press(Message::ToggleLog),
        );
        if self.log_expanded {
            // Fixed height matches window boost so the form does not jump up.
            let log_scroll = scrollable(
                text_editor(&self.log_content)
                    .font(Font::MONOSPACE)
                    .size(12)
                    .height(Fill)
                    .on_action(Message::LogAction),
            )
            .height(Fill)
            .direction(ScrollDirection::Vertical(
                Scrollbar::new()
                    .width(12)
                    .scroller_width(12)
                    .spacing(4),
            ));
            body = body.push(
                container(log_scroll)
                    .padding(8)
                    .width(Fill)
                    .height(Length::Fixed(LOG_PANE_HEIGHT))
                    .style(|_theme| container::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb8(
                            36, 40, 48,
                        ))),
                        border: iced::Border {
                            color: iced::Color::from_rgb8(90, 98, 112),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }

        body.into()
    }

    fn view_contacts(&self) -> Element<'_, Message> {
        let file_row = row![
            button(text("File…").size(UI_BODY_SIZE))
                .on_press_maybe((!self.running).then_some(Message::PickValidateFile)),
            text_input(".vcf or .csv", &self.validate_input)
                .on_input(Message::ValidatePath)
                .size(UI_BODY_SIZE)
                .padding(6)
                .width(Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let radios = column![
            radio(
                "USA",
                true,
                Some(self.validate_usa),
                Message::ValidateUsa,
            )
            .size(16)
            .text_size(UI_BODY_SIZE),
            radio(
                "International",
                false,
                Some(self.validate_usa),
                Message::ValidateUsa,
            )
            .size(16)
            .text_size(UI_BODY_SIZE),
        ]
        .spacing(6);

        let has_file = !self.validate_input.trim().is_empty();
        let can_check = !self.running && has_file;
        let already_updated = self
            .last_updated_input
            .as_ref()
            .is_some_and(|p| p == self.validate_input.trim());
        let can_update = can_check && !already_updated;
        let mut actions = row![].spacing(8);
        actions = actions.push(
            button(text("Check").size(UI_BODY_SIZE))
                .on_press_maybe(can_check.then_some(Message::Check)),
        );
        actions = actions.push(
            button(text("Format").size(UI_BODY_SIZE))
                .on_press_maybe(can_update.then_some(Message::Update)),
        );
        if self.running {
            actions = actions.push(
                button(text("Cancel").size(UI_BODY_SIZE)).on_press(Message::Cancel),
            );
        }

        column![
            rule::horizontal(1),
            text("Phone Numbers").size(UI_TITLE_SIZE),
            Space::new().height(8),
            text("Contacts file").size(UI_BODY_SIZE),
            container(file_row).padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 12.0,
            }),
            Space::new().height(10),
            text("Phone number format").size(UI_BODY_SIZE),
            container(radios).padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 12.0,
            }),
            Space::new().height(16),
            row![space().width(Fill), actions],
        ]
        .spacing(6)
        .into()
    }

    fn view_export(&self) -> Element<'_, Message> {
        // Fields below the pinned Export title scroll; tabs/title never do.
        let mut fields = Column::new()
            .spacing(8)
            .push(text("Global options").size(UI_BODY_SIZE))
            .push(
                checkbox(self.form.anonymize)
                    .label("Anonymize")
                    .size(16)
                    .text_size(UI_BODY_SIZE)
                    .on_toggle(Message::Anonymize),
            );

        if self.form.anonymize || !self.form.anonymize_seed.is_empty() {
            fields = fields.push(labeled_input(
                "Seed",
                "Optional 64-hex seed",
                &self.form.anonymize_seed,
                Message::Seed,
            ));
        }
        fields = fields
            .push(labeled_input(
                "Start date",
                "YYYY-MM-DD",
                &self.form.start_date,
                Message::StartDate,
            ))
            .push(labeled_input(
                "End date",
                "YYYY-MM-DD",
                &self.form.end_date,
                Message::EndDate,
            ))
            .push(
                row![
                    text("Backup source").size(UI_BODY_SIZE).width(130),
                    pick_list(
                        EXPORTERS.as_slice(),
                        Some(self.exporter),
                        Message::ExporterSelected,
                    )
                    .text_size(UI_BODY_SIZE)
                    .width(220),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .push(rule::horizontal(1))
            .push(
                button(text(self.exporter.link_label()).size(UI_BODY_SIZE))
                    .style(button::text)
                    .on_press(Message::OpenProductUrl),
            );

        if self.exporter == Exporter::Imessage {
            fields = fields
                .push(path_row(
                    "Database / iOS backup path",
                    &self.form.db_path,
                    Message::DbPath,
                    Some(Message::PickDbPath),
                    Some(Message::PickDbFolder),
                ))
                .push(
                    row![
                        text("Backup password").size(UI_BODY_SIZE).width(130),
                        text_input("Encrypted iOS backup password", &self.form.backup_password)
                            .secure(true)
                            .on_input(Message::BackupPassword)
                            .size(UI_BODY_SIZE)
                            .padding(6)
                            .width(Fill),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .push(
                    row![
                        text("Platform").size(UI_BODY_SIZE).width(130),
                        pick_list(
                            APPLE_PLATFORMS.as_slice(),
                            Some(self.form.apple_platform),
                            Message::ApplePlatform,
                        )
                        .text_size(UI_BODY_SIZE)
                        .width(220),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .push(path_row(
                    "Output directory",
                    &self.form.output,
                    Message::Output,
                    None,
                    Some(Message::PickOutputFolder),
                ));
            fields = fields.push(self.view_attachment_media());
            fields = fields.push(
                button(
                    text(if self.form.advanced {
                        "▾ Hide advanced options"
                    } else {
                        "▸ Show advanced options"
                    })
                    .size(UI_BODY_SIZE),
                )
                .on_press(Message::ToggleAdvanced),
            );
            if self.form.advanced {
                fields = fields
                    .push(path_row(
                        "Attachment root",
                        &self.form.attachment_root,
                        Message::AttachmentRoot,
                        None,
                        Some(Message::PickAttachmentRoot),
                    ))
                    .push(labeled_input(
                        "Conversation filter",
                        "Names, numbers, or emails",
                        &self.form.conversation_filter,
                        Message::ConversationFilter,
                    ))
                    .push(path_row(
                        "Apple AddressBook DB",
                        &self.form.apple_contacts,
                        Message::AppleContacts,
                        Some(Message::PickAppleContacts),
                        None,
                    ));
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
            fields = fields
                .push(path_row(
                    input_label,
                    &self.form.input,
                    Message::Input,
                    file.then_some(Message::PickInputFile),
                    folder.then_some(Message::PickInputFolder),
                ))
                .push(path_row(
                    "Output directory",
                    &self.form.output,
                    Message::Output,
                    None,
                    Some(Message::PickOutputFolder),
                ));

            if matches!(
                self.exporter,
                Exporter::GoSmsPro | Exporter::SmsBackupRestore | Exporter::SmsBackupPlus
            ) {
                fields = fields
                    .push(labeled_input(
                        "Your phone number(s)",
                        "Comma-separated phone numbers",
                        &self.form.owner_phones,
                        Message::OwnerPhones,
                    ))
                    .push(self.view_attachment_media());
            }
            if self.exporter == Exporter::SmsBackupPlus {
                fields = fields.push(labeled_input(
                    "Your email address(es)",
                    "Comma-separated email addresses",
                    &self.form.owner_emails,
                    Message::OwnerEmails,
                ));
            }

            fields = fields.push(self.view_contacts_fields());

            if self.exporter == Exporter::Imazing {
                fields = fields.push(labeled_input(
                    "Timezone",
                    "IANA name, e.g. America/New_York",
                    &self.form.timezone,
                    Message::Timezone,
                ));
            }

            if self.exporter == Exporter::SmsBackupPlus {
                fields = fields.push(
                    button(
                        text(if self.form.advanced {
                            "▾ Hide advanced options"
                        } else {
                            "▸ Show advanced options"
                        })
                        .size(UI_BODY_SIZE),
                    )
                    .on_press(Message::ToggleAdvanced),
                );
                if self.form.advanced {
                    fields = fields.push(path_row(
                        "Name mapping CSV",
                        &self.form.name_mapping,
                        Message::NameMapping,
                        Some(Message::PickNameMapping),
                        None,
                    ));
                }
            }
        }

        let actions = row![
            space().width(Fill),
            button(text("Run exporter").size(UI_BODY_SIZE))
                .on_press_maybe((!self.running).then_some(Message::RunExport)),
            button(text("Clear").size(UI_BODY_SIZE)).on_press(Message::ClearExport),
        ]
        .spacing(8);
        fields = fields.push(Space::new().height(8)).push(actions);

        column![
            rule::horizontal(1),
            text("Export").size(UI_TITLE_SIZE),
            text("Convert phone backups into readable conversation CSV")
                .size(UI_BODY_SIZE)
                .color(iced::Color::from_rgb8(160, 160, 160)),
            scrollable(fields).height(Fill),
        ]
        .spacing(8)
        .height(Fill)
        .into()
    }

    fn view_contacts_fields(&self) -> Element<'_, Message> {
        if self.exporter == Exporter::Imazing {
            return path_row(
                "iMazing Contacts CSV (recommended)",
                &self.form.contacts,
                Message::Contacts,
                Some(Message::PickContactsFile),
                None,
            );
        }
        path_row(
            "Contacts",
            &self.form.contacts,
            Message::Contacts,
            Some(Message::PickContactsFile),
            None,
        )
    }

    fn view_attachment_media(&self) -> Element<'_, Message> {
        let mut col = Column::new().spacing(8).push(
            row![
                text("Attachments").size(UI_BODY_SIZE).width(130),
                pick_list(
                    ATTACHMENT_MEDIA.as_slice(),
                    Some(self.form.attachment_media),
                    Message::AttachmentMedia,
                )
                .text_size(UI_BODY_SIZE)
                .width(260),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        if self.form.attachment_media.needs_ffmpeg() && !message_media::ffmpeg_available() {
            col = col.push(
                text("Convert/Compress need ffmpeg and ffprobe on PATH.")
                    .size(UI_BODY_SIZE)
                    .color(iced::Color::from_rgb8(180, 50, 50)),
            );
        }
        if self.form.attachment_media == AttachmentMedia::Compress {
            col = col
                .push(
                    row![
                        text("Max resolution").size(UI_BODY_SIZE).width(130),
                        pick_list(
                            MAX_RESOLUTIONS.as_slice(),
                            Some(self.form.media_max_resolution),
                            Message::MaxResolution,
                        )
                        .text_size(UI_BODY_SIZE)
                        .width(160),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .push(labeled_input(
                    "Max fps",
                    "e.g. 30",
                    &self.form.media_max_fps,
                    Message::MaxFps,
                ))
                .push(labeled_input(
                    "Min size",
                    "e.g. 20M",
                    &self.form.media_min_size,
                    Message::MinSize,
                ))
                .push(
                    checkbox(self.form.media_skip_efficient)
                        .label("Skip already-efficient HEVC")
                        .size(16)
                        .text_size(UI_BODY_SIZE)
                        .on_toggle(Message::SkipEfficient),
                );
        }
        col.into()
    }

    fn poll_events(&mut self) {
        let events: Vec<ProcessEvent> = {
            let Some(rx) = &self.rx else {
                return;
            };
            let mut events = Vec::new();
            while let Ok(event) = rx.try_recv() {
                let terminal = matches!(event, ProcessEvent::Finished(_) | ProcessEvent::Error(_));
                events.push(event);
                if terminal {
                    break;
                }
            }
            events
        };
        if events.is_empty() {
            return;
        }

        let mut finished_imessage = false;
        for event in events {
            match event {
                ProcessEvent::Started(command) => {
                    self.push_log(format!("Running: {command}"));
                }
                ProcessEvent::Log(line) => self.push_log(line),
                ProcessEvent::Finished(summary) => {
                    self.push_log(summary);
                    finished_imessage =
                        self.mode == AppMode::Message && self.exporter == Exporter::Imessage;
                    if self.pending_validate_update {
                        self.last_updated_input =
                            Some(self.validate_input.trim().to_string());
                    }
                    self.pending_validate_update = false;
                    self.running = false;
                    self.rx = None;
                }
                ProcessEvent::Error(error) => {
                    self.errors = vec![error.clone()];
                    self.push_log(format!("Error: {error}"));
                    self.pending_validate_update = false;
                    self.running = false;
                    self.rx = None;
                }
            }
        }
        if finished_imessage {
            self.run_imessage_media_post();
        }
    }

    fn set_validate_input(&mut self, path: String) {
        let trimmed = path.trim().to_string();
        if self
            .last_updated_input
            .as_ref()
            .is_some_and(|locked| locked != &trimmed)
        {
            self.last_updated_input = None;
        }
        self.validate_input = path;
    }

    fn start_export(&mut self) -> Task<Message> {
        if self.running {
            return Task::none();
        }
        let start = self.form.start_date.trim();
        let end = self.form.end_date.trim();
        if let Err(error) = DateRange::parse(
            (!start.is_empty()).then_some(start),
            (!end.is_empty()).then_some(end),
        ) {
            let grow = self.set_log_expanded(true);
            self.begin_session_log();
            self.push_log(format!("Invalid date range: {error}"));
            return grow;
        }
        let args = match self.form.build_args(self.exporter) {
            Ok(args) => args,
            Err(errors) => {
                self.errors = errors;
                return Task::none();
            }
        };
        let output = std::path::PathBuf::from(self.form.output.trim());
        if let Err(error) = ensure_output_dir(&output) {
            let grow = self.set_log_expanded(true);
            self.begin_session_log();
            self.errors = vec![error.clone()];
            self.push_log(format!("Error: {error}"));
            return grow;
        }
        let program = match resolve_binary(self.exporter.binary()) {
            Ok(program) => program,
            Err(error) => {
                self.errors = vec![error];
                return Task::none();
            }
        };
        self.errors.clear();
        self.running = true;
        self.pending_validate_update = false;
        let grow = self.set_log_expanded(true);
        self.begin_session_log();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        spawn(program, args, self.control.clone(), tx);
        grow
    }

    fn sync_default_output(&mut self, old_input: &str, new_input: &str) {
        let previous_default = default_output_dir(self.exporter, old_input);
        if self.form.output.trim().is_empty() || self.form.output == previous_default {
            self.form.output = default_output_dir(self.exporter, new_input);
        }
    }

    /// Set contacts path and infer CSV/VCF kind from extension; log if invalid.
    fn set_contacts_path(&mut self, path: String) -> Task<Message> {
        let trimmed = path.trim().to_string();
        if trimmed.is_empty() {
            self.form.contacts.clear();
            self.form.contacts_kind = ContactsKind::None;
            return Task::none();
        }
        let ext = std::path::Path::new(&trimmed)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "csv" => {
                self.form.contacts = trimmed;
                self.form.contacts_kind = ContactsKind::Csv;
                Task::none()
            }
            "vcf" | "vcard" => {
                self.form.contacts = trimmed;
                self.form.contacts_kind = ContactsKind::Vcf;
                Task::none()
            }
            _ => {
                self.form.contacts = trimmed;
                self.form.contacts_kind = ContactsKind::None;
                let grow = self.set_log_expanded(true);
                self.ensure_session_log();
                self.push_log(
                    "Contacts file must be .csv or .vcf (path kept but will not be used)."
                        .into(),
                );
                grow
            }
        }
    }

    /// Reset export form fields; keep selected Backup source / exporter.
    fn clear_export_form(&mut self) -> Task<Message> {
        let exporter = self.exporter;
        self.form = Form {
            output: default_output_dir(exporter, ""),
            ..Form::default()
        };
        self.errors.clear();
        Task::none()
    }

    fn start_validate(&mut self, check_only: bool) -> Task<Message> {
        if self.running {
            return Task::none();
        }
        let input = self.validate_input.trim();
        if input.is_empty() {
            self.errors = vec!["Choose a contacts CSV or VCF file.".into()];
            return Task::none();
        }
        if let Err(error) = message_contacts::probe_contacts_input(std::path::Path::new(input)) {
            self.errors = vec![error.message.clone()];
            let grow = self.set_log_expanded(true);
            self.begin_session_log();
            self.push_log(format!("# validate preflight failed: {}", error.message));
            for line in &error.details {
                self.push_log(line.clone());
            }
            return grow;
        }

        let program = match resolve_binary("contacts-validate") {
            Ok(program) => program,
            Err(error) => {
                self.errors = vec![error];
                return Task::none();
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
        self.pending_validate_update = !check_only;
        let grow = self.set_log_expanded(true);
        self.begin_session_log();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        spawn(program, args, self.control.clone(), tx);
        grow
    }

    /// Expand/collapse the log pane: grow on open, restore prior height on roll-up.
    fn set_log_expanded(&mut self, expanded: bool) -> Task<Message> {
        if expanded == self.log_expanded {
            return Task::none();
        }
        self.log_expanded = expanded;
        if expanded {
            self.sync_log_content();
            if self.window_height_before_log.is_some() {
                return Task::none();
            }
            return iced::window::latest().and_then(|id| {
                iced::window::size(id).map(Message::LogWindowBoost)
            });
        }
        let restore = self.window_height_before_log.take();
        resize_window_to_height(restore)
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
        self.sync_log_content();
    }

    fn push_log(&mut self, line: String) {
        self.ensure_session_log();
        if let Some(path) = &self.session_log_path {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{line}");
            }
        }
        self.logs.push(line);
        self.sync_log_content();
    }

    fn sync_log_content(&mut self) {
        let text = if self.logs.is_empty() {
            LOG_PLACEHOLDER.to_string()
        } else {
            self.logs.join("\n")
        };
        self.log_content = text_editor::Content::with_text(&text);
    }

    fn cancel(&mut self) {
        match self.control.cancel() {
            Ok(()) => self.push_log("Cancellation requested…".into()),
            Err(error) => self.errors = vec![error],
        }
    }

    fn run_imessage_media_post(&mut self) {
        let mode = self.form.attachment_media.media_mode();
        if matches!(mode, MediaMode::Disabled) {
            return;
        }
        let output = std::path::PathBuf::from(self.form.output.trim());
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
}

/// Two-tone tab icons: solid dark circle with a white cutout glyph.
fn phone_numbers_icon_handle() -> SvgHandle {
    // Classic handset silhouette (white) on black circle.
    SvgHandle::from_memory(
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <circle cx="12" cy="12" r="11" fill="#111827"/>
  <path fill="#FFFFFF" d="M8.1 5.8c.5-.5 1.3-.6 1.9-.3l1.6.8c.5.3.8.9.6 1.5l-.5 1.7c-.1.4 0 .8.3 1.1l2.4 2.4c.3.3.7.4 1.1.3l1.7-.5c.6-.2 1.2.1 1.5.6l.8 1.6c.3.6.2 1.4-.3 1.9l-.9.9c-.6.6-1.4.9-2.2.8-2.1-.2-4.5-1.5-6.7-3.7S5.6 10.5 5.4 8.4c-.1-.8.2-1.6.8-2.2l.9-.9z"/>
</svg>"##,
    )
}

fn message_export_icon_handle() -> SvgHandle {
    // External-link / export arrow (white) on black circle.
    SvgHandle::from_memory(
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <circle cx="12" cy="12" r="11" fill="#111827"/>
  <path fill="none" stroke="#FFFFFF" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"
    d="M8.2 10.2V15.8a1 1 0 0 0 1 1h5.6M10 8.2h5.8v5.8M10.6 13.4L15.8 8.2"/>
</svg>"##,
    )
}

fn tab_button<'a>(
    icon: SvgHandle,
    label: &'a str,
    active: bool,
    msg: Message,
) -> Element<'a, Message> {
    let content = column![
        svg(icon).width(28).height(28),
        text(label).size(11),
    ]
    .spacing(2)
    .align_x(Alignment::Center);
    let btn = button(content).on_press(msg).padding([8, 12]);
    if active {
        btn.style(button::primary).into()
    } else {
        btn.style(button::secondary).into()
    }
}

fn labeled_input<'a>(
    label: &'a str,
    placeholder: &'a str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        text(label).size(UI_BODY_SIZE).width(130),
        text_input(placeholder, value)
            .on_input(on_input)
            .size(UI_BODY_SIZE)
            .padding(6)
            .width(Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn export_input_path<'a>(exporter: Exporter, form: &'a Form) -> &'a str {
    if exporter == Exporter::Imessage {
        form.db_path.as_str()
    } else {
        form.input.as_str()
    }
}

fn path_row<'a>(
    label: &'a str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    file_msg: Option<Message>,
    folder_msg: Option<Message>,
) -> Element<'a, Message> {
    let mut r = row![text(label).size(UI_BODY_SIZE).width(130),]
        .spacing(6)
        .align_y(Alignment::Center);
    r = r.push(
        text_input("Path", value)
            .on_input(on_input)
            .size(UI_BODY_SIZE)
            .padding(6)
            .width(Fill),
    );
    if let Some(msg) = file_msg {
        r = r.push(button(text("File…").size(UI_BODY_SIZE)).on_press(msg));
    }
    if let Some(msg) = folder_msg {
        r = r.push(button(text("Folder…").size(UI_BODY_SIZE)).on_press(msg));
    }
    r.into()
}

/// Restore compact height on roll-up (`stored`), or subtract the expand boost.
fn resize_window_to_height(stored: Option<f32>) -> Task<Message> {
    iced::window::latest().and_then(move |id| {
        iced::window::size(id).then(move |size| {
            let height = stored
                .unwrap_or_else(|| size.height - LOG_PANE_HEIGHT)
                .max(WINDOW_MIN_HEIGHT);
            iced::window::resize(id, iced::Size::new(size.width, height))
        })
    })
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

fn pick_file(extensions: Option<&[&str]>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(exts) = extensions {
        dialog = dialog.add_filter("Contacts", exts);
    }
    dialog.pick_file().map(|p| p.display().to_string())
}

fn pick_folder() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|p| p.display().to_string())
}
