//! In-process exporter job adapters for the GUI.

use std::sync::mpsc;

use go_sms_pro_exporter::run as run_go_sms_pro;
use imazing_exporter::run as run_imazing;
use imessage_ir_exporter::run as run_imessage;
use message_exporters_core::{CancelFlag, Exporter, ExporterConfig, ProcessEvent};
use openextract_exporter::run as run_openextract;
use sms_backup_plus_exporter::run as run_sms_plus;
use sms_backup_restore_exporter::run as run_sms_restore;
use whatsapp_exporter::run as run_whatsapp;

pub(crate) type LibraryJob =
    Box<dyn FnOnce(CancelFlag, mpsc::Sender<ProcessEvent>) -> Result<(), String> + Send>;

/// Build an in-process export job from a validated [`ExporterConfig`].
pub(crate) fn library_job_for_exporter(exporter: Exporter, config: ExporterConfig) -> LibraryJob {
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

pub(crate) fn run_and_log<R, E: std::fmt::Display>(
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

pub(crate) trait HasMessages {
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
