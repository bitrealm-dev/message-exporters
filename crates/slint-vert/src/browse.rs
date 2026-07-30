//! Native file/folder pickers via `rfd`, run off the Slint UI thread so the
//! event loop is never blocked (Wayland compositors treat a blocked UI as hung).

use std::path::PathBuf;

use slint::{ComponentHandle, SharedString, Weak};

use crate::AppWindow;
use crate::ContactsAdapter;
use crate::ConvertAdapter;
use crate::ExportAdapter;
use crate::VaultAdapter;

#[derive(Debug, Clone, Copy)]
pub enum BrowseKind {
    File,
    Folder,
    FileOrFolder,
}

pub fn browse_kind_for_field(field_id: &str) -> BrowseKind {
    match field_id {
        "contacts.input"
        | "export.contacts"
        | "export.name_mapping"
        | "export.whatsapp_wa"
        | "export.whatsapp_db"
        | "export.apple_contacts" => BrowseKind::File,
        "export.input" => BrowseKind::FileOrFolder,
        "export.db_path" => BrowseKind::FileOrFolder,
        "export.whatsapp_backup" => BrowseKind::FileOrFolder,
        _ => BrowseKind::Folder,
    }
}

/// Spawn a background dialog, then apply the picked path on the UI thread.
pub fn pick_path(ui_weak: Weak<AppWindow>, field_id: String, kind: BrowseKind) {
    std::thread::spawn(move || {
        let dialog = rfd::FileDialog::new().set_title("Choose path");
        let picked: Option<PathBuf> = match kind {
            BrowseKind::File => dialog.pick_file(),
            BrowseKind::Folder => dialog.pick_folder(),
            BrowseKind::FileOrFolder => dialog
                .pick_folder()
                .or_else(|| rfd::FileDialog::new().set_title("Choose file").pick_file()),
        };
        let Some(path) = picked else {
            return;
        };
        let path = SharedString::from(path.display().to_string());
        let _ = ui_weak.upgrade_in_event_loop(move |ui| {
            apply_path(&ui, &field_id, path);
        });
    });
}

fn apply_path(ui: &AppWindow, field_id: &str, path: SharedString) {
    match field_id {
        "contacts.input" => ui.global::<ContactsAdapter>().set_input(path),
        "export.input" => ui.global::<ExportAdapter>().set_input(path),
        "export.output" => ui.global::<ExportAdapter>().set_output(path),
        "export.db_path" => ui.global::<ExportAdapter>().set_db_path(path),
        "export.contacts" => ui.global::<ExportAdapter>().set_contacts(path),
        "export.name_mapping" => ui.global::<ExportAdapter>().set_name_mapping(path),
        "export.whatsapp_backup" => ui.global::<ExportAdapter>().set_whatsapp_backup(path),
        "export.whatsapp_wa" => ui.global::<ExportAdapter>().set_whatsapp_wa(path),
        "export.whatsapp_media" => ui.global::<ExportAdapter>().set_whatsapp_media(path),
        "export.whatsapp_db" => ui.global::<ExportAdapter>().set_whatsapp_db(path),
        "export.apple_contacts" => ui.global::<ExportAdapter>().set_apple_contacts(path),
        "export.attachment_root" => ui.global::<ExportAdapter>().set_attachment_root(path),
        "convert.input" => ui.global::<ConvertAdapter>().set_input(path),
        "convert.output" => ui.global::<ConvertAdapter>().set_output(path),
        "vault.input" => ui.global::<VaultAdapter>().set_input(path),
        _ => {}
    }
}
