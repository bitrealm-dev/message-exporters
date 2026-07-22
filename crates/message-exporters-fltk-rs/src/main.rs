//! FLTK front-end for message-exporters (Contacts validate + Message export).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};

use fltk::{
    app,
    button::{Button, CheckButton, RadioRoundButton},
    dialog::{FileDialogType, NativeFileChooser},
    enums::{Color, FrameType},
    group::{Flex, Scroll, Tabs},
    input::{Input, SecretInput},
    menu::Choice,
    output::MultilineOutput,
    prelude::*,
    text::{TextBuffer, TextDisplay},
    window::Window,
};
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_exporters_core::{
    default_output_dir, resolve_binary, spawn, ApplePlatform, AttachmentMedia, ContactsKind,
    Exporter, Form, ProcessControl, ProcessEvent, APPLE_PLATFORMS, ATTACHMENT_MEDIA, CONTACT_KINDS,
    EXPORTERS, MAX_RESOLUTIONS,
};
use message_media::{process_near_vault_media, MaxResolution, MediaMode};

#[derive(Clone, Copy, Debug)]
enum UiMsg {
    Check,
    Update,
    RunExport,
    Cancel,
    PickValidateFile,
    PickInputFile,
    PickInputFolder,
    PickOutputFolder,
    PickContactsFile,
    PickNameMapping,
    PickDbPath,
    PickDbFolder,
    PickAttachmentRoot,
    PickAppleContacts,
    ExporterChanged,
    ContactsKindChanged,
    AttachmentMediaChanged,
    ToggleAdvanced,
    OpenProductUrl,
    ClearLog,
    Tick,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppMode {
    Contacts,
    Message,
}

struct AppState {
    mode: AppMode,
    exporter: Exporter,
    form: Form,
    validate_usa: bool,
    running: bool,
    control: ProcessControl,
    logs: Vec<String>,
    errors: Vec<String>,
    rx: Option<Receiver<ProcessEvent>>,
}

impl Default for AppState {
    fn default() -> Self {
        let exporter = Exporter::default();
        Self {
            mode: AppMode::Contacts,
            exporter,
            form: Form {
                output: default_output_dir(exporter),
                ..Form::default()
            },
            validate_usa: true,
            running: false,
            control: ProcessControl::default(),
            logs: Vec::new(),
            errors: Vec::new(),
            rx: None,
        }
    }
}

struct Widgets {
    validate_path: Input,
    radio_usa: RadioRoundButton,
    radio_intl: RadioRoundButton,
    btn_check: Button,
    btn_update: Button,
    btn_cancel_contacts: Button,

    anonymize: CheckButton,
    seed: Input,
    start_date: Input,
    end_date: Input,
    exporter_choice: Choice,
    product_link: Button,

    input_path: Input,
    output_path: Input,
    owner_phones: Input,
    owner_emails: Input,
    contacts_kind: Choice,
    contacts_path: Input,
    timezone: Input,
    name_mapping: Input,

    db_path: Input,
    backup_password: SecretInput,
    apple_platform: Choice,
    attachment_root: Input,
    conversation_filter: Input,
    apple_contacts: Input,

    attachment_media: Choice,
    max_resolution: Choice,
    max_fps: Input,
    min_size: Input,
    skip_efficient: CheckButton,
    btn_advanced: Button,
    btn_run: Button,
    btn_cancel_export: Button,

    // Visibility groups (Flex rows) keyed by purpose
    row_seed: Flex,
    row_input: Flex,
    row_owner_phones: Flex,
    row_owner_emails: Flex,
    row_contacts_kind: Flex,
    row_contacts_path: Flex,
    row_timezone: Flex,
    row_name_mapping: Flex,
    row_db: Flex,
    row_password: Flex,
    row_platform: Flex,
    row_attachment_root: Flex,
    row_conversation: Flex,
    row_apple_contacts: Flex,
    row_media: Flex,
    row_compress: Flex,
    row_ffmpeg_warn: Flex,

    errors: TextDisplay,
    errors_buf: TextBuffer,
    log: MultilineOutput,
    status: fltk::frame::Frame,
}

fn main() {
    let app = app::App::default().with_scheme(app::Scheme::Gtk);
    let (s, r) = app::channel::<UiMsg>();

    let state = Rc::new(RefCell::new(AppState::default()));

    let mut wind = Window::default()
        .with_size(640, 520)
        .with_label("Message Exporters");
    wind.make_resizable(true);
    wind.size_range(480, 360, 0, 0);

    let mut root = Flex::default_fill().column();
    root.set_margin(0);
    root.set_pad(0);

    let mut tabs = Tabs::default_fill();
    tabs.set_tab_align(fltk::enums::Align::Center);

    // --- Contacts tab ---
    let mut contacts_tab = Flex::default_fill()
        .with_label("Contacts")
        .column();
    contacts_tab.set_margin(18);
    contacts_tab.set_pad(8);

    let mut heading = fltk::frame::Frame::default().with_label("Validate Contacts");
    heading.set_label_size(18);
    heading.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    contacts_tab.fixed(&heading, 28);

    let mut lbl_file = fltk::frame::Frame::default().with_label("Contacts file");
    lbl_file.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    contacts_tab.fixed(&lbl_file, 20);

    let mut file_row = Flex::default().row();
    file_row.set_pad(8);
    let mut btn_file = Button::default().with_label("File…");
    file_row.fixed(&btn_file, 70);
    let mut validate_path = Input::default();
    validate_path.set_tooltip(".vcf or .csv");
    file_row.end();
    contacts_tab.fixed(&file_row, 28);

    let mut lbl_fmt = fltk::frame::Frame::default().with_label("Phone number format");
    lbl_fmt.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    contacts_tab.fixed(&lbl_fmt, 20);

    let mut radio_col = Flex::default().column();
    radio_col.set_pad(4);
    let mut radio_usa = RadioRoundButton::default().with_label("USA");
    radio_usa.set_value(true);
    radio_col.fixed(&radio_usa, 24);
    let radio_intl = RadioRoundButton::default().with_label("International");
    radio_col.fixed(&radio_intl, 24);
    radio_col.end();
    contacts_tab.fixed(&radio_col, 52);

    let spacer = fltk::frame::Frame::default();
    let _ = spacer;

    let mut action_row = Flex::default().row();
    action_row.set_pad(8);
    let _action_spacer = fltk::frame::Frame::default();
    let mut btn_check = Button::default().with_label("Check");
    action_row.fixed(&btn_check, 80);
    let mut btn_update = Button::default().with_label("Update");
    btn_update.deactivate();
    action_row.fixed(&btn_update, 80);
    let mut btn_cancel_contacts = Button::default().with_label("Cancel");
    btn_cancel_contacts.deactivate();
    action_row.fixed(&btn_cancel_contacts, 80);
    action_row.end();
    contacts_tab.fixed(&action_row, 32);

    contacts_tab.end();

    // --- Message tab ---
    let mut message_tab = Flex::default_fill()
        .with_label("Message")
        .column();
    message_tab.set_margin(0);
    message_tab.set_pad(0);

    let mut scroll = Scroll::default_fill();
    scroll.set_frame(FrameType::NoBox);

    let mut form_col = Flex::default()
        .with_size(600, 900)
        .column();
    form_col.set_margin(12);
    form_col.set_pad(6);

    let mut export_heading = fltk::frame::Frame::default().with_label("Export");
    export_heading.set_label_size(18);
    export_heading.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    form_col.fixed(&export_heading, 26);

    let mut global_lbl = fltk::frame::Frame::default().with_label("Global options");
    global_lbl.set_label_size(14);
    global_lbl.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    form_col.fixed(&global_lbl, 22);

    let mut anonymize = CheckButton::default().with_label("Anonymize");
    form_col.fixed(&anonymize, 24);

    let mut row_seed = labeled_input_row("Seed", "Optional 64-hex seed");
    let seed = row_seed.1.clone();
    form_col.fixed(&row_seed.0, 28);
    row_seed.0.hide();

    let row_start = labeled_input_row("Start date", "YYYY-MM-DD");
    let start_date = row_start.1.clone();
    form_col.fixed(&row_start.0, 28);

    let row_end = labeled_input_row("End date", "YYYY-MM-DD");
    let end_date = row_end.1.clone();
    form_col.fixed(&row_end.0, 28);

    let mut exporter_row = Flex::default().row();
    exporter_row.set_pad(8);
    let mut src_lbl = fltk::frame::Frame::default().with_label("Backup source");
    src_lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    exporter_row.fixed(&src_lbl, 120);
    let mut exporter_choice = Choice::default();
    for exporter in EXPORTERS {
        exporter_choice.add_choice(exporter.display_name());
    }
    exporter_choice.set_value(0);
    exporter_row.end();
    form_col.fixed(&exporter_row, 28);

    let mut product_link = Button::default().with_label(EXPORTERS[0].link_label());
    product_link.set_frame(FrameType::NoBox);
    product_link.set_label_color(Color::from_hex(0x3B82F6));
    form_col.fixed(&product_link, 24);

    // Generic input/output
    let row_input = path_row("Input source");
    let input_path = row_input.1.clone();
    let mut btn_input_file = row_input.2;
    let mut btn_input_folder = row_input.3;
    form_col.fixed(&row_input.0, 28);

    let mut row_output = path_row("Output directory");
    let mut output_path = row_output.1.clone();
    output_path.set_value(&default_output_dir(Exporter::default()));
    let mut btn_output_folder = row_output.3;
    row_output.2.hide();
    form_col.fixed(&row_output.0, 28);

    let row_owner_phones = labeled_input_row("Your phone number(s)", "Comma-separated");
    let owner_phones = row_owner_phones.1.clone();
    form_col.fixed(&row_owner_phones.0, 28);

    let mut row_owner_emails = labeled_input_row("Your email address(es)", "Comma-separated");
    let owner_emails = row_owner_emails.1.clone();
    form_col.fixed(&row_owner_emails.0, 28);
    row_owner_emails.0.hide();

    let mut row_contacts_kind = Flex::default().row();
    row_contacts_kind.set_pad(8);
    let mut ck_lbl = fltk::frame::Frame::default().with_label("Contacts");
    ck_lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    row_contacts_kind.fixed(&ck_lbl, 120);
    let mut contacts_kind = Choice::default();
    for kind in CONTACT_KINDS {
        contacts_kind.add_choice(&kind.to_string());
    }
    contacts_kind.set_value(0);
    row_contacts_kind.end();
    form_col.fixed(&row_contacts_kind, 28);

    let mut row_contacts_path = path_row("Contacts file");
    let contacts_path = row_contacts_path.1.clone();
    let mut btn_contacts_file = row_contacts_path.2;
    row_contacts_path.3.hide();
    form_col.fixed(&row_contacts_path.0, 28);
    row_contacts_path.0.hide();

    let mut row_timezone = labeled_input_row("Timezone", "e.g. America/New_York");
    let timezone = row_timezone.1.clone();
    form_col.fixed(&row_timezone.0, 28);
    row_timezone.0.hide();

    // iMessage fields
    let mut row_db = path_row("Database / iOS backup path");
    let db_path = row_db.1.clone();
    let mut btn_db_file = row_db.2;
    let mut btn_db_folder = row_db.3;
    form_col.fixed(&row_db.0, 28);
    row_db.0.hide();

    let mut row_password = Flex::default().row();
    row_password.set_pad(8);
    let mut pw_lbl = fltk::frame::Frame::default().with_label("Backup password");
    pw_lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    row_password.fixed(&pw_lbl, 120);
    let backup_password = SecretInput::default();
    row_password.end();
    form_col.fixed(&row_password, 28);
    row_password.hide();

    let mut row_platform = Flex::default().row();
    row_platform.set_pad(8);
    let mut plat_lbl = fltk::frame::Frame::default().with_label("Platform");
    plat_lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    row_platform.fixed(&plat_lbl, 120);
    let mut apple_platform = Choice::default();
    for p in APPLE_PLATFORMS {
        apple_platform.add_choice(&p.to_string());
    }
    apple_platform.set_value(0);
    row_platform.end();
    form_col.fixed(&row_platform, 28);
    row_platform.hide();

    let mut row_media = Flex::default().row();
    row_media.set_pad(8);
    let mut media_lbl = fltk::frame::Frame::default().with_label("Attachments");
    media_lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    row_media.fixed(&media_lbl, 120);
    let mut attachment_media = Choice::default();
    for m in ATTACHMENT_MEDIA {
        attachment_media.add_choice(&m.to_string());
    }
    attachment_media.set_value(0);
    row_media.end();
    form_col.fixed(&row_media, 28);

    let mut row_ffmpeg_warn = Flex::default().row();
    let mut ffmpeg_warn = fltk::frame::Frame::default()
        .with_label("Convert/Compress need ffmpeg and ffprobe on PATH.");
    ffmpeg_warn.set_label_color(Color::from_hex(0xB43232));
    ffmpeg_warn.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    row_ffmpeg_warn.end();
    form_col.fixed(&row_ffmpeg_warn, 22);
    row_ffmpeg_warn.hide();

    let mut row_compress = Flex::default().column();
    row_compress.set_pad(4);
    let mut res_row = Flex::default().row();
    res_row.set_pad(8);
    let mut res_lbl = fltk::frame::Frame::default().with_label("Max resolution");
    res_lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    res_row.fixed(&res_lbl, 120);
    let mut max_resolution = Choice::default();
    for r in MAX_RESOLUTIONS {
        max_resolution.add_choice(&r.to_string());
    }
    max_resolution.set_value(0);
    res_row.end();
    row_compress.fixed(&res_row, 28);
    let fps_row = labeled_input_row("Max fps", "e.g. 30");
    let mut max_fps = fps_row.1.clone();
    max_fps.set_value("30");
    row_compress.fixed(&fps_row.0, 28);
    let size_row = labeled_input_row("Min size", "e.g. 20M");
    let mut min_size = size_row.1.clone();
    min_size.set_value("20M");
    row_compress.fixed(&size_row.0, 28);
    let mut skip_efficient = CheckButton::default().with_label("Skip already-efficient HEVC");
    skip_efficient.set_value(true);
    row_compress.fixed(&skip_efficient, 24);
    row_compress.end();
    form_col.fixed(&row_compress, 120);
    row_compress.hide();

    let mut btn_advanced = Button::default().with_label("▸ Show advanced options");
    form_col.fixed(&btn_advanced, 28);
    btn_advanced.hide();

    let mut row_attachment_root = path_row("Attachment root");
    let attachment_root = row_attachment_root.1.clone();
    let mut btn_att_root = row_attachment_root.3;
    row_attachment_root.2.hide();
    form_col.fixed(&row_attachment_root.0, 28);
    row_attachment_root.0.hide();

    let mut row_conversation = labeled_input_row("Conversation filter", "Names, numbers, or emails");
    let conversation_filter = row_conversation.1.clone();
    form_col.fixed(&row_conversation.0, 28);
    row_conversation.0.hide();

    let mut row_apple_contacts = path_row("Apple AddressBook DB");
    let apple_contacts = row_apple_contacts.1.clone();
    let mut btn_apple_contacts = row_apple_contacts.2;
    row_apple_contacts.3.hide();
    form_col.fixed(&row_apple_contacts.0, 28);
    row_apple_contacts.0.hide();

    let mut row_name_mapping = path_row("Name mapping CSV");
    let name_mapping = row_name_mapping.1.clone();
    let mut btn_name_mapping = row_name_mapping.2;
    row_name_mapping.3.hide();
    form_col.fixed(&row_name_mapping.0, 28);
    row_name_mapping.0.hide();

    let mut export_actions = Flex::default().row();
    export_actions.set_pad(8);
    let mut btn_run = Button::default().with_label("Run exporter");
    export_actions.fixed(&btn_run, 120);
    let mut btn_cancel_export = Button::default().with_label("Cancel");
    btn_cancel_export.deactivate();
    export_actions.fixed(&btn_cancel_export, 80);
    let _ea_spacer = fltk::frame::Frame::default();
    export_actions.end();
    form_col.fixed(&export_actions, 32);

    form_col.end();
    scroll.end();
    message_tab.end();

    tabs.end();
    tabs.auto_layout();
    root.fixed(&tabs, -1);

    // Errors + log + status
    let errors_buf = TextBuffer::default();
    let mut errors = TextDisplay::default();
    errors.set_buffer(errors_buf.clone());
    errors.set_text_color(Color::from_hex(0x8C2828));
    errors.set_frame(FrameType::FlatBox);
    errors.set_color(Color::from_rgb(255, 235, 235));
    root.fixed(&errors, 0);
    errors.hide();

    let mut log_row = Flex::default().row();
    log_row.set_pad(4);
    let mut log_lbl = fltk::frame::Frame::default().with_label("Run log");
    log_lbl.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    log_row.fixed(&log_lbl, 70);
    let mut btn_clear = Button::default().with_label("Clear");
    log_row.fixed(&btn_clear, 60);
    let _log_sp = fltk::frame::Frame::default();
    log_row.end();
    root.fixed(&log_row, 24);

    let mut log = MultilineOutput::default();
    log.set_text_font(fltk::enums::Font::Courier);
    log.set_text_size(11);
    root.fixed(&log, 120);

    let mut status = fltk::frame::Frame::default();
    status.set_frame(FrameType::EngravedBox);
    status.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    root.fixed(&status, 22);

    root.end();
    wind.end();
    wind.show();

    let widgets = Rc::new(RefCell::new(Widgets {
        validate_path: validate_path.clone(),
        radio_usa: radio_usa.clone(),
        radio_intl: radio_intl.clone(),
        btn_check: btn_check.clone(),
        btn_update: btn_update.clone(),
        btn_cancel_contacts: btn_cancel_contacts.clone(),
        anonymize: anonymize.clone(),
        seed: seed.clone(),
        start_date: start_date.clone(),
        end_date: end_date.clone(),
        exporter_choice: exporter_choice.clone(),
        product_link: product_link.clone(),
        input_path: input_path.clone(),
        output_path: output_path.clone(),
        owner_phones: owner_phones.clone(),
        owner_emails: owner_emails.clone(),
        contacts_kind: contacts_kind.clone(),
        contacts_path: contacts_path.clone(),
        timezone: timezone.clone(),
        name_mapping: name_mapping.clone(),
        db_path: db_path.clone(),
        backup_password: backup_password.clone(),
        apple_platform: apple_platform.clone(),
        attachment_root: attachment_root.clone(),
        conversation_filter: conversation_filter.clone(),
        apple_contacts: apple_contacts.clone(),
        attachment_media: attachment_media.clone(),
        max_resolution: max_resolution.clone(),
        max_fps: max_fps.clone(),
        min_size: min_size.clone(),
        skip_efficient: skip_efficient.clone(),
        btn_advanced: btn_advanced.clone(),
        btn_run: btn_run.clone(),
        btn_cancel_export: btn_cancel_export.clone(),
        row_seed: row_seed.0.clone(),
        row_input: row_input.0.clone(),
        row_owner_phones: row_owner_phones.0.clone(),
        row_owner_emails: row_owner_emails.0.clone(),
        row_contacts_kind: row_contacts_kind.clone(),
        row_contacts_path: row_contacts_path.0.clone(),
        row_timezone: row_timezone.0.clone(),
        row_name_mapping: row_name_mapping.0.clone(),
        row_db: row_db.0.clone(),
        row_password: row_password.clone(),
        row_platform: row_platform.clone(),
        row_attachment_root: row_attachment_root.0.clone(),
        row_conversation: row_conversation.0.clone(),
        row_apple_contacts: row_apple_contacts.0.clone(),
        row_media: row_media.clone(),
        row_compress: row_compress.clone(),
        row_ffmpeg_warn: row_ffmpeg_warn.clone(),
        errors: errors.clone(),
        errors_buf: errors_buf.clone(),
        log: log.clone(),
        status: status.clone(),
    }));

    // Wire callbacks
    btn_file.emit(s, UiMsg::PickValidateFile);
    btn_check.emit(s, UiMsg::Check);
    btn_update.emit(s, UiMsg::Update);
    btn_cancel_contacts.emit(s, UiMsg::Cancel);
    btn_run.emit(s, UiMsg::RunExport);
    btn_cancel_export.emit(s, UiMsg::Cancel);
    btn_clear.emit(s, UiMsg::ClearLog);
    btn_advanced.emit(s, UiMsg::ToggleAdvanced);
    product_link.emit(s, UiMsg::OpenProductUrl);
    exporter_choice.emit(s, UiMsg::ExporterChanged);
    contacts_kind.emit(s, UiMsg::ContactsKindChanged);
    attachment_media.emit(s, UiMsg::AttachmentMediaChanged);
    btn_input_file.emit(s, UiMsg::PickInputFile);
    btn_input_folder.emit(s, UiMsg::PickInputFolder);
    btn_output_folder.emit(s, UiMsg::PickOutputFolder);
    btn_contacts_file.emit(s, UiMsg::PickContactsFile);
    btn_name_mapping.emit(s, UiMsg::PickNameMapping);
    btn_db_file.emit(s, UiMsg::PickDbPath);
    btn_db_folder.emit(s, UiMsg::PickDbFolder);
    btn_att_root.emit(s, UiMsg::PickAttachmentRoot);
    btn_apple_contacts.emit(s, UiMsg::PickAppleContacts);

    {
        let s = s;
        validate_path.set_callback(move |inp| {
            let has = !inp.value().trim().is_empty();
            // Enable update via tick-driven refresh; send Tick to refresh buttons
            s.send(UiMsg::Tick);
            let _ = has;
        });
    }
    {
        let s = s;
        anonymize.set_callback(move |_| s.send(UiMsg::Tick));
    }

    // Track active tab
    {
        let state = state.clone();
        tabs.set_callback(move |t| {
            if let Some(val) = t.value() {
                let mut st = state.borrow_mut();
                st.mode = if val.label().starts_with("Message") {
                    AppMode::Message
                } else {
                    AppMode::Contacts
                };
            }
        });
    }

    refresh_exporter_visibility(&mut widgets.borrow_mut(), &state.borrow());
    app::add_timeout3(0.1, move |handle| {
        s.send(UiMsg::Tick);
        app::repeat_timeout3(0.1, handle);
    });

    while app.wait() {
        while let Some(msg) = r.recv() {
            handle_msg(msg, &state, &widgets);
        }
    }
}

fn labeled_input_row(label: &str, tooltip: &str) -> (Flex, Input) {
    let mut row = Flex::default().row();
    row.set_pad(8);
    let mut lbl = fltk::frame::Frame::default().with_label(label);
    lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    row.fixed(&lbl, 120);
    let mut input = Input::default();
    input.set_tooltip(tooltip);
    row.end();
    (row, input)
}

fn path_row(label: &str) -> (Flex, Input, Button, Button) {
    let mut row = Flex::default().row();
    row.set_pad(6);
    let mut lbl = fltk::frame::Frame::default().with_label(label);
    lbl.set_align(fltk::enums::Align::Right | fltk::enums::Align::Inside);
    row.fixed(&lbl, 120);
    let input = Input::default();
    let btn_file = Button::default().with_label("File…");
    row.fixed(&btn_file, 60);
    let btn_folder = Button::default().with_label("Folder…");
    row.fixed(&btn_folder, 70);
    row.end();
    (row, input, btn_file, btn_folder)
}

fn pick_file(filter: Option<&str>) -> Option<String> {
    let mut dialog = NativeFileChooser::new(FileDialogType::BrowseFile);
    if let Some(filter) = filter {
        dialog.set_filter(filter);
    }
    dialog.show();
    let path = dialog.filename();
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.display().to_string())
    }
}

fn pick_folder() -> Option<String> {
    let mut dialog = NativeFileChooser::new(FileDialogType::BrowseDir);
    dialog.show();
    let path = dialog.filename();
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.display().to_string())
    }
}

fn handle_msg(msg: UiMsg, state: &Rc<RefCell<AppState>>, widgets: &Rc<RefCell<Widgets>>) {
    match msg {
        UiMsg::Tick => {
            poll_process(state, widgets);
            refresh_ui(state, widgets);
        }
        UiMsg::ClearLog => {
            state.borrow_mut().logs.clear();
            refresh_ui(state, widgets);
        }
        UiMsg::PickValidateFile => {
            if let Some(path) = pick_file(Some("*.{csv,vcf,vcard}\nContacts")) {
                widgets.borrow_mut().validate_path.set_value(&path);
                refresh_ui(state, widgets);
            }
        }
        UiMsg::PickInputFile => {
            if let Some(path) = pick_file(None) {
                widgets.borrow_mut().input_path.set_value(&path);
            }
        }
        UiMsg::PickInputFolder | UiMsg::PickOutputFolder | UiMsg::PickDbFolder
        | UiMsg::PickAttachmentRoot => {
            if let Some(path) = pick_folder() {
                let mut w = widgets.borrow_mut();
                match msg {
                    UiMsg::PickInputFolder => w.input_path.set_value(&path),
                    UiMsg::PickOutputFolder => w.output_path.set_value(&path),
                    UiMsg::PickDbFolder => w.db_path.set_value(&path),
                    UiMsg::PickAttachmentRoot => w.attachment_root.set_value(&path),
                    _ => {}
                }
            }
        }
        UiMsg::PickContactsFile | UiMsg::PickNameMapping | UiMsg::PickAppleContacts
        | UiMsg::PickDbPath => {
            let filter = matches!(msg, UiMsg::PickContactsFile | UiMsg::PickNameMapping)
                .then_some("*.{csv,vcf,vcard}\nContacts");
            if let Some(path) = pick_file(filter) {
                let mut w = widgets.borrow_mut();
                match msg {
                    UiMsg::PickContactsFile => w.contacts_path.set_value(&path),
                    UiMsg::PickNameMapping => w.name_mapping.set_value(&path),
                    UiMsg::PickAppleContacts => w.apple_contacts.set_value(&path),
                    UiMsg::PickDbPath => w.db_path.set_value(&path),
                    _ => {}
                }
            }
        }
        UiMsg::Check => start_validate(state, widgets, true),
        UiMsg::Update => start_validate(state, widgets, false),
        UiMsg::RunExport => start_export(state, widgets),
        UiMsg::Cancel => {
            let mut st = state.borrow_mut();
            match st.control.cancel() {
                Ok(()) => st.logs.push("Cancellation requested…".into()),
                Err(error) => st.errors = vec![error],
            }
            drop(st);
            refresh_ui(state, widgets);
        }
        UiMsg::ExporterChanged => {
            let mut st = state.borrow_mut();
            let mut w = widgets.borrow_mut();
            let idx = w.exporter_choice.value().max(0) as usize;
            let previous = st.exporter;
            if let Some(exporter) = EXPORTERS.get(idx).copied() {
                st.exporter = exporter;
                let previous_default = default_output_dir(previous);
                let current_out = w.output_path.value();
                if current_out.trim().is_empty() || current_out == previous_default {
                    w.output_path.set_value(&default_output_dir(exporter));
                }
                st.form.advanced = false;
                st.errors.clear();
                w.product_link.set_label(exporter.link_label());
                refresh_exporter_visibility(&mut w, &st);
            }
        }
        UiMsg::ContactsKindChanged | UiMsg::AttachmentMediaChanged | UiMsg::ToggleAdvanced => {
            let mut st = state.borrow_mut();
            let mut w = widgets.borrow_mut();
            if matches!(msg, UiMsg::ToggleAdvanced) {
                st.form.advanced = !st.form.advanced;
            }
            refresh_exporter_visibility(&mut w, &st);
        }
        UiMsg::OpenProductUrl => {
            let url = state.borrow().exporter.product_url();
            if let Err(error) = open::that(url) {
                state.borrow_mut().errors = vec![format!("Could not open link: {error}")];
                refresh_ui(state, widgets);
            }
        }
    }
}

fn sync_form_from_widgets(st: &mut AppState, w: &Widgets) {
    st.validate_usa = w.radio_usa.value() && !w.radio_intl.value();
    st.form.anonymize = w.anonymize.value();
    st.form.anonymize_seed = w.seed.value();
    st.form.start_date = w.start_date.value();
    st.form.end_date = w.end_date.value();
    st.form.input = w.input_path.value();
    st.form.output = w.output_path.value();
    st.form.owner_phones = w.owner_phones.value();
    st.form.owner_emails = w.owner_emails.value();
    st.form.contacts = w.contacts_path.value();
    st.form.timezone = w.timezone.value();
    st.form.name_mapping = w.name_mapping.value();
    st.form.db_path = w.db_path.value();
    st.form.backup_password = w.backup_password.value();
    st.form.attachment_root = w.attachment_root.value();
    st.form.conversation_filter = w.conversation_filter.value();
    st.form.apple_contacts = w.apple_contacts.value();
    st.form.media_max_fps = w.max_fps.value();
    st.form.media_min_size = w.min_size.value();
    st.form.media_skip_efficient = w.skip_efficient.value();

    let ck = w.contacts_kind.value().max(0) as usize;
    st.form.contacts_kind = CONTACT_KINDS.get(ck).copied().unwrap_or_default();
    let media = w.attachment_media.value().max(0) as usize;
    st.form.attachment_media = ATTACHMENT_MEDIA.get(media).copied().unwrap_or_default();
    let res = w.max_resolution.value().max(0) as usize;
    st.form.media_max_resolution = MAX_RESOLUTIONS
        .get(res)
        .copied()
        .unwrap_or(MaxResolution::default());
    let plat = w.apple_platform.value().max(0) as usize;
    st.form.apple_platform = APPLE_PLATFORMS.get(plat).copied().unwrap_or(ApplePlatform::Auto);
}

fn start_validate(state: &Rc<RefCell<AppState>>, widgets: &Rc<RefCell<Widgets>>, check_only: bool) {
    let mut st = state.borrow_mut();
    if st.running {
        return;
    }
    let w = widgets.borrow();
    let input = w.validate_path.value();
    let input = input.trim();
    if input.is_empty() {
        st.errors = vec!["Choose a contacts CSV or VCF file.".into()];
        drop(w);
        drop(st);
        refresh_ui(state, widgets);
        return;
    }
    if let Err(error) = message_contacts::probe_contacts_input(std::path::Path::new(input)) {
        st.errors = vec![error.message];
        drop(w);
        drop(st);
        refresh_ui(state, widgets);
        return;
    }
    let program = match resolve_binary("contacts-validate") {
        Ok(p) => p,
        Err(error) => {
            st.errors = vec![error];
            drop(w);
            drop(st);
            refresh_ui(state, widgets);
            return;
        }
    };
    let region = if w.radio_usa.value() {
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
    st.errors.clear();
    st.running = true;
    st.logs.clear();
    let (tx, rx) = mpsc::channel();
    st.rx = Some(rx);
    spawn(program, args, st.control.clone(), tx);
    drop(w);
    drop(st);
    refresh_ui(state, widgets);
}

fn start_export(state: &Rc<RefCell<AppState>>, widgets: &Rc<RefCell<Widgets>>) {
    let mut st = state.borrow_mut();
    if st.running {
        return;
    }
    {
        let w = widgets.borrow();
        sync_form_from_widgets(&mut st, &w);
    }
    let args = match st.form.build_args(st.exporter) {
        Ok(args) => args,
        Err(errors) => {
            st.errors = errors;
            drop(st);
            refresh_ui(state, widgets);
            return;
        }
    };
    let program = match resolve_binary(st.exporter.binary()) {
        Ok(p) => p,
        Err(error) => {
            st.errors = vec![error];
            drop(st);
            refresh_ui(state, widgets);
            return;
        }
    };
    st.errors.clear();
    st.running = true;
    st.logs.clear();
    let (tx, rx) = mpsc::channel();
    st.rx = Some(rx);
    spawn(program, args, st.control.clone(), tx);
    drop(st);
    refresh_ui(state, widgets);
}

fn poll_process(state: &Rc<RefCell<AppState>>, widgets: &Rc<RefCell<Widgets>>) {
    let events: Vec<ProcessEvent> = {
        let st = state.borrow();
        let Some(rx) = &st.rx else {
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
    {
        let mut st = state.borrow_mut();
        for event in events {
            match event {
                ProcessEvent::Started(command) => {
                    st.logs.push(format!("Running: {command}"));
                }
                ProcessEvent::Log(line) => st.logs.push(line),
                ProcessEvent::Finished(summary) => {
                    st.logs.push(summary);
                    finished_imessage =
                        st.mode == AppMode::Message && st.exporter == Exporter::Imessage;
                    st.running = false;
                    st.rx = None;
                }
                ProcessEvent::Error(error) => {
                    st.errors = vec![error.clone()];
                    st.logs.push(format!("Error: {error}"));
                    st.running = false;
                    st.rx = None;
                }
            }
        }
    }
    if finished_imessage {
        run_imessage_media_post(state);
    }
    refresh_ui(state, widgets);
}

fn run_imessage_media_post(state: &Rc<RefCell<AppState>>) {
    let mut st = state.borrow_mut();
    let mode = st.form.attachment_media.media_mode();
    if matches!(mode, MediaMode::Disabled) {
        return;
    }
    let output = std::path::PathBuf::from(st.form.output.trim());
    if mode.needs_tools() {
        st.logs
            .push(format!("Processing attachment media ({mode})…"));
        let compress = match st.form.compress_options() {
            Ok(opts) => opts,
            Err(error) => {
                st.errors = vec![error.clone()];
                st.logs.push(format!("Error: {error}"));
                return;
            }
        };
        match process_near_vault_media(&output, mode, &compress) {
            Ok(report) => {
                if report.processed > 0 || report.skipped > 0 || !report.errors.is_empty() {
                    st.logs.push(format!(
                        "Media: processed {} file(s), skipped {}, updated {} CSV(s)",
                        report.processed, report.skipped, report.csv_files_updated
                    ));
                }
                for err in report.errors.iter().take(10) {
                    st.logs.push(format!("media warning: {err}"));
                }
            }
            Err(error) => {
                let msg = format!("Media processing failed: {error}");
                st.errors = vec![msg.clone()];
                st.logs.push(msg);
                return;
            }
        }
    }
    if mode.needs_tools() && (st.form.anonymize || !st.form.anonymize_seed.trim().is_empty()) {
        let seed = {
            let s = st.form.anonymize_seed.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        };
        match resolve_anonymizer(seed.as_deref())
            .and_then(|mut anon| anonymize_near_vault_dir(&output, &mut anon).map(|n| (n, anon)))
        {
            Ok((n, _)) => st
                .logs
                .push(format!("Anonymized {n} CSV file(s) under {}", output.display())),
            Err(error) => {
                let msg = format!("Anonymize failed: {error}");
                st.errors = vec![msg.clone()];
                st.logs.push(msg);
            }
        }
    }
}

fn refresh_ui(state: &Rc<RefCell<AppState>>, widgets: &Rc<RefCell<Widgets>>) {
    let st = state.borrow();
    let mut w = widgets.borrow_mut();

    let has_validate = !w.validate_path.value().trim().is_empty();
    if st.running {
        w.btn_check.deactivate();
        w.btn_update.deactivate();
        w.btn_run.deactivate();
        w.btn_cancel_contacts.activate();
        w.btn_cancel_export.activate();
    } else {
        w.btn_check.activate();
        if has_validate {
            w.btn_update.activate();
        } else {
            w.btn_update.deactivate();
        }
        w.btn_run.activate();
        w.btn_cancel_contacts.deactivate();
        w.btn_cancel_export.deactivate();
    }

    if w.anonymize.value() || !w.seed.value().is_empty() {
        w.row_seed.show();
    } else {
        w.row_seed.hide();
    }

    if st.errors.is_empty() {
        w.errors_buf.set_text("");
        w.errors.hide();
    } else {
        let text = st
            .errors
            .iter()
            .map(|e| format!("• {e}"))
            .collect::<Vec<_>>()
            .join("\n");
        w.errors_buf.set_text(&text);
        w.errors.show();
    }

    w.log.set_value(&st.logs.join("\n"));
    let status = st
        .logs
        .last()
        .cloned()
        .unwrap_or_else(|| if st.running { "Running…".into() } else { String::new() });
    w.status.set_label(&status);

    // Ensure parent layouts recompute after show/hide
    if let Some(mut win) = w.status.window() {
        win.redraw();
    }
}

fn refresh_exporter_visibility(w: &mut Widgets, st: &AppState) {
    let exporter = st.exporter;
    let advanced = st.form.advanced;
    let media_idx = w.attachment_media.value().max(0) as usize;
    let media = ATTACHMENT_MEDIA
        .get(media_idx)
        .copied()
        .unwrap_or_default();
    let contacts_idx = w.contacts_kind.value().max(0) as usize;
    let contacts_kind = CONTACT_KINDS
        .get(contacts_idx)
        .copied()
        .unwrap_or_default();

    let is_imessage = exporter == Exporter::Imessage;
    let is_android = matches!(
        exporter,
        Exporter::GoSmsPro | Exporter::SmsBackupRestore | Exporter::SmsBackupPlus
    );
    let is_plus = exporter == Exporter::SmsBackupPlus;
    let is_imazing = exporter == Exporter::Imazing;

    set_visible(&mut w.row_input, !is_imessage);
    set_visible(&mut w.row_db, is_imessage);
    set_visible(&mut w.row_password, is_imessage);
    set_visible(&mut w.row_platform, is_imessage);
    set_visible(&mut w.row_owner_phones, is_android);
    set_visible(&mut w.row_owner_emails, is_plus);
    set_visible(&mut w.row_timezone, is_imazing);
    set_visible(&mut w.row_media, is_android || is_imessage);
    set_visible(
        &mut w.row_ffmpeg_warn,
        (is_android || is_imessage) && media.needs_ffmpeg() && !message_media::ffmpeg_available(),
    );
    set_visible(
        &mut w.row_compress,
        (is_android || is_imessage) && media == AttachmentMedia::Compress,
    );

    if is_imazing {
        set_visible(&mut w.row_contacts_kind, false);
        set_visible(&mut w.row_contacts_path, true);
    } else if is_imessage {
        set_visible(&mut w.row_contacts_kind, false);
        set_visible(&mut w.row_contacts_path, false);
    } else {
        set_visible(&mut w.row_contacts_kind, true);
        set_visible(
            &mut w.row_contacts_path,
            contacts_kind != ContactsKind::None,
        );
    }

    let show_adv_btn = is_imessage || is_plus;
    if show_adv_btn {
        w.btn_advanced.show();
        w.btn_advanced.set_label(if advanced {
            "▾ Hide advanced options"
        } else {
            "▸ Show advanced options"
        });
    } else {
        w.btn_advanced.hide();
    }

    set_visible(&mut w.row_attachment_root, is_imessage && advanced);
    set_visible(&mut w.row_conversation, is_imessage && advanced);
    set_visible(&mut w.row_apple_contacts, is_imessage && advanced);
    set_visible(&mut w.row_name_mapping, is_plus && advanced);

    // Input pickers: Go SMS Pro folder-only
    // (buttons remain; File hidden for GoSmsPro via deactivate of file if needed)
    let _ = exporter;
}

fn set_visible(widget: &mut impl WidgetExt, visible: bool) {
    if visible {
        widget.show();
    } else {
        widget.hide();
    }
}
