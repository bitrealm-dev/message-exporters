//! Open macOS chat.db or iOS backup (encrypted → temp sms.db).

use std::{
    fs::remove_file,
    path::{Path, PathBuf},
};

use crabapple::Backup;
use imessage_database::{tables::table::get_connection, util::platform::Platform};
use rusqlite::Connection;

use crate::{
    backup::{decrypt_backup, get_decrypted_contacts_database, get_decrypted_message_database},
    contacts::{ContactsIndex, DEFAULT_PATH_IOS},
    error::RuntimeError,
    options::MailOptions,
};

struct TempDatabase(PathBuf);

impl TempDatabase {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        if let Err(why) = remove_file(&self.0) {
            eprintln!(
                "warning: failed to remove temporary messages database at {}: {why}",
                self.0.display(),
            );
        }
    }
}

pub struct DataSource {
    messages_connection: Option<Connection>,
    pub contacts_index: ContactsIndex,
    pub backup: Option<Backup>,
    temp_messages_db: Option<TempDatabase>,
}

impl DataSource {
    pub fn from(options: &MailOptions) -> Result<Self, RuntimeError> {
        match options.platform {
            Platform::macOS => {
                let messages_path = options.get_db_path();
                let contacts_index =
                    Self::get_contacts_index(options.contacts_path.as_deref()).unwrap_or_default();

                Ok(Self {
                    messages_connection: Some(get_connection(&messages_path)?),
                    contacts_index,
                    backup: None,
                    temp_messages_db: None,
                })
            }
            Platform::iOS => match decrypt_backup(options)? {
                Some(backup) => {
                    let messages_db = TempDatabase(get_decrypted_message_database(&backup)?);
                    let contacts_path = get_decrypted_contacts_database(&backup)?;

                    eprintln!(
                        "Decrypted iOS backup: {} (version {})\n",
                        backup.lockdown().device_name,
                        backup.lockdown().product_version,
                    );

                    let contacts_index =
                        Self::get_contacts_index(Some(&contacts_path)).unwrap_or_default();

                    if let Err(e) = remove_file(&contacts_path) {
                        eprintln!(
                            "warning: failed to remove temporary contacts database at {}: {e}",
                            contacts_path.display()
                        );
                    }

                    let messages_connection = get_connection(messages_db.path())?;
                    Ok(Self {
                        messages_connection: Some(messages_connection),
                        contacts_index,
                        backup: Some(backup),
                        temp_messages_db: Some(messages_db),
                    })
                }
                None => {
                    let messages_path = options.get_db_path();
                    let contacts_index =
                        Self::get_contacts_index(Some(&options.db_path.join(DEFAULT_PATH_IOS)))
                            .unwrap_or_default();

                    Ok(Self {
                        messages_connection: Some(get_connection(&messages_path)?),
                        contacts_index,
                        backup: None,
                        temp_messages_db: None,
                    })
                }
            },
        }
    }

    fn get_contacts_index(path: Option<&Path>) -> Option<ContactsIndex> {
        match ContactsIndex::build(path) {
            Ok(index) => Some(index),
            Err(e) => {
                eprintln!(
                    "Unable to build contacts index: {e}\nContinuing without contact names..."
                );
                None
            }
        }
    }

    pub fn db(&self) -> &Connection {
        match self.messages_connection.as_ref() {
            Some(db) => db,
            None => panic!("Database connection is closed!"),
        }
    }
}

impl Drop for DataSource {
    fn drop(&mut self) {
        if let Some(conn) = self.messages_connection.take() {
            conn.close().ok();
        }
        drop(self.temp_messages_db.take());
    }
}
