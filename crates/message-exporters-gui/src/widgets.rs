//! Shared egui form widgets and layout helpers.

use eframe::egui;

pub(crate) const LABEL_W: f32 = 190.0;
pub(crate) const PATH_W: f32 = 400.0;
pub(crate) const COMBO_W: f32 = 200.0;
pub(crate) const SHORT_W: f32 = 140.0;
pub(crate) const MIN_FIELD_W: f32 = 160.0;
pub(crate) const PICKER_BUTTON_W: f32 = 72.0;
/// First row plus up to 9 added rows.
pub(crate) const MAX_OWNER_PHONES: usize = 10;

pub(crate) fn form_label(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) {
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

pub(crate) fn required_field_label(ui: &egui::Ui, text: &str) -> egui::text::LayoutJob {
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

pub(crate) fn required_field_note(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("* Required field")
            .small()
            .color(ui.visuals().weak_text_color()),
    );
}

pub(crate) fn form_action_row(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        form_label(ui, "");
        add(ui);
    });
}

pub(crate) fn form_action_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(label).min_size(egui::vec2(88.0, ui.spacing().interact_size.y)),
    )
}

pub(crate) fn advanced_options_toggle(ui: &mut egui::Ui, advanced: &mut bool) {
    form_action_row(ui, |ui| {
        if ui
            .button(if *advanced {
                "▾ Hide advanced options"
            } else {
                "▸ Show advanced options"
            })
            .clicked()
        {
            *advanced = !*advanced;
        }
    });
}

pub(crate) fn required_value_rows(
    ui: &mut egui::Ui,
    rows: &mut Vec<String>,
    id_prefix: &'static str,
    label: &str,
    hint: &str,
    item_name: &str,
    item_name_plural: &str,
) {
    if rows.is_empty() {
        rows.push(String::new());
    }
    let mut remove_idx = None;
    let mut add_row = false;
    let row_count = rows.len();
    for i in 0..row_count {
        ui.horizontal(|ui| {
            let width = responsive_field_width(ui, PATH_W, 1);
            if i == 0 {
                let label = required_field_label(ui, label);
                form_label(ui, label);
            } else {
                ui.allocate_exact_size(
                    egui::vec2(LABEL_W, ui.spacing().interact_size.y),
                    egui::Sense::hover(),
                );
            }
            with_field_width(ui, width, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut rows[i])
                        .id_salt((id_prefix, i))
                        .desired_width(width)
                        .clip_text(true)
                        .hint_text(hint),
                );
            });
            if i == 0 {
                let can_add = row_count < MAX_OWNER_PHONES;
                let add = ui
                    .add_enabled(can_add, egui::Button::new("+"))
                    .on_hover_text(if can_add {
                        format!("Add {item_name}")
                    } else {
                        format!("Maximum of {MAX_OWNER_PHONES} {item_name_plural}")
                    });
                if add.clicked() {
                    add_row = true;
                }
            } else if ui
                .button("−")
                .on_hover_text(format!("Remove {item_name}"))
                .clicked()
            {
                remove_idx = Some(i);
            }
        });
    }
    if add_row && rows.len() < MAX_OWNER_PHONES {
        rows.push(String::new());
    }
    if let Some(i) = remove_idx {
        if i > 0 && i < rows.len() {
            rows.remove(i);
        }
    }
}

pub(crate) fn responsive_field_width(
    ui: &egui::Ui,
    max_width: f32,
    trailing_buttons: usize,
) -> f32 {
    let spacing = ui.spacing().item_spacing.x;
    let trailing_width = trailing_buttons as f32 * (PICKER_BUTTON_W + spacing) + LABEL_W + spacing;
    (ui.available_width() - trailing_width)
        .max(MIN_FIELD_W.min(max_width))
        .min(max_width)
}

/// Reserve an exact field width so sibling controls cannot shrink it unexpectedly.
pub(crate) fn with_field_width(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        add,
    );
}

pub(crate) fn labeled_text(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    width: f32,
) {
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

pub(crate) fn required_labeled_text(
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

pub(crate) fn path_or_text(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    allow_file: bool,
    allow_folder: bool,
) {
    path_or_text_labeled(ui, label, label, value, hint, allow_file, allow_folder);
}

pub(crate) fn required_path_or_text(
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

pub(crate) fn path_or_text_labeled(
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

pub(crate) fn combo_enum<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[T],
    width: f32,
) {
    combo_enum_with_id(ui, label, label, value, options, width);
}

pub(crate) fn combo_enum_with_id<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut egui::Ui,
    label: &str,
    id_salt: &str,
    value: &mut T,
    options: &[T],
    width: f32,
) {
    ui.horizontal(|ui| {
        let width = responsive_field_width(ui, width, 0);
        form_label(ui, label);
        with_field_width(ui, width, |ui| {
            egui::ComboBox::from_id_salt(id_salt)
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
