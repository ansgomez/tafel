//! Crash backups (Tafel).
//!
//! While a document has unsaved changes, a snapshot is written every [`BACKUP_INTERVAL_SECS`] to
//! `<stem>.backup.rnote` next to the edited file: the `.rnote` file, or the PDF when the document came
//! from a PDF and was never saved. A normal save and a clean close delete the backup. When a file is
//! opened and a differing backup exists, the user is asked whether to restore it.

// Imports
use crate::{RnAppWindow, RnCanvas};
use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk4::glib::{self, clone};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use tracing::{debug, error};

/// How often pending changes are written to the backup file.
pub(crate) const BACKUP_INTERVAL_SECS: u32 = 120;
const BACKUP_SUFFIX: &str = ".backup.rnote";

/// The failure toast is shown only once per session.
static FAILURE_TOAST_SHOWN: AtomicBool = AtomicBool::new(false);

/// Per-canvas backup state.
#[derive(Debug, Default)]
pub(crate) struct BackupState {
    /// The document changed since the last backup write.
    pending: bool,
    /// Bumped whenever the backup is deleted, so that a write that was in flight at that time removes its file again.
    epoch: u64,
    /// The PDF the document was created from, used while the document has no `.rnote` file.
    source_pdf: Option<PathBuf>,
}

/// The backup file path for the edited file `source`.
///
/// Returns `None` for files that are backups themselves, so a backup never gets its own backup.
pub(crate) fn backup_path(source: &Path) -> Option<PathBuf> {
    let name = source.file_name()?;
    if name.to_string_lossy().ends_with(BACKUP_SUFFIX) {
        return None;
    }
    let mut backup_name = OsString::from(source.file_stem()?);
    backup_name.push(BACKUP_SUFFIX);
    Some(source.with_file_name(backup_name))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FoundBackup {
    None,
    /// Same content as the opened file.
    Identical,
    Differs {
        bytes: Vec<u8>,
        modified: Option<SystemTime>,
    },
}

/// Look for a backup at `backup` and compare it with `opened`, if given. Blocking.
pub(crate) fn find_backup(backup: &Path, opened: Option<&Path>) -> std::io::Result<FoundBackup> {
    let bytes = match std::fs::read(backup) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FoundBackup::None),
        Err(e) => return Err(e),
    };
    if let Some(opened) = opened
        && std::fs::read(opened)? == bytes
    {
        return Ok(FoundBackup::Identical);
    }
    let modified = std::fs::metadata(backup).and_then(|m| m.modified()).ok();
    Ok(FoundBackup::Differs { bytes, modified })
}

/// Delete the backup file, ignoring a missing file.
fn remove_backup_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => debug!("Removed backup file {path:?}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => error!("Removing backup file {path:?} failed, Err: {e:?}"),
    }
}

impl RnCanvas {
    /// The file the backup is placed next to: the output file, else the source PDF.
    pub(crate) fn backup_file(&self) -> Option<PathBuf> {
        let source = self
            .output_file()
            .and_then(|f| f.path())
            .or_else(|| self.imp().backup.borrow().source_pdf.clone())?;
        backup_path(&source)
    }

    pub(crate) fn set_backup_pending(&self, pending: bool) {
        self.imp().backup.borrow_mut().pending = pending;
    }

    pub(crate) fn set_backup_source_pdf(&self, source_pdf: Option<PathBuf>) {
        self.imp().backup.borrow_mut().source_pdf = source_pdf;
    }

    /// Delete the backup at `path` (if any) and stop a write that is in flight from leaving a file behind.
    pub(crate) fn discard_backup(&self, path: Option<PathBuf>) {
        {
            let mut state = self.imp().backup.borrow_mut();
            state.pending = false;
            state.epoch += 1;
        }
        if let Some(path) = path {
            remove_backup_file(&path);
        }
    }

    /// Whether a backup should be written now.
    fn backup_due(&self) -> bool {
        self.imp().backup.borrow().pending && self.unsaved_changes() && !self.save_in_progress()
    }

    /// Serialize the document like a normal save does and write it atomically to the backup file.
    async fn write_backup(&self) -> anyhow::Result<()> {
        let Some(path) = self.backup_file() else {
            return Ok(());
        };
        let epoch = {
            let mut state = self.imp().backup.borrow_mut();
            state.pending = false;
            state.epoch
        };
        // Never write an empty document over a backup that has content.
        if self.engine_ref().has_no_strokes() {
            return Ok(());
        }
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        // Bind the receiver first so the engine is not borrowed across the await.
        let bytes_receiver = self.engine_ref().save_as_rnote_bytes(file_name);
        let result = async {
            let bytes = bytes_receiver.await??;
            crate::utils::atomic_save_to_file_future(&path, bytes).await
        }
        .await;
        if let Err(e) = result {
            // Try again on the next interval.
            self.set_backup_pending(true);
            return Err(e);
        }
        if self.imp().backup.borrow().epoch != epoch {
            // Saved or closed while writing.
            remove_backup_file(&path);
        } else {
            debug!("Wrote backup file {path:?}");
        }
        Ok(())
    }
}

impl RnAppWindow {
    /// Start the periodic backup timer. It stops with the window.
    pub(crate) fn setup_backups(&self) {
        glib::timeout_add_seconds_local(
            BACKUP_INTERVAL_SECS,
            clone!(
                #[weak(rename_to = appwindow)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    appwindow.write_due_backups();
                    glib::ControlFlow::Continue
                }
            ),
        );
    }

    fn backups_enabled(&self) -> bool {
        self.sidebar()
            .settings_panel()
            .general_backups_row()
            .is_active()
    }

    fn write_due_backups(&self) {
        if !self.backups_enabled() {
            return;
        }
        for canvas in self.get_all_tabs().iter().map(|tab| tab.canvas()) {
            if !canvas.backup_due() {
                continue;
            }
            glib::spawn_future_local(clone!(
                #[weak]
                canvas,
                #[weak(rename_to = appwindow)]
                self,
                async move {
                    if let Err(e) = canvas.write_backup().await {
                        error!("Writing backup file failed, Err: {e:?}");
                        if !FAILURE_TOAST_SHOWN.swap(true, Ordering::Relaxed) {
                            appwindow.overlays().dispatch_toast_text(
                                &gettext("Could not write the backup file"),
                                crate::overlays::TEXT_TOAST_TIMEOUT_DEFAULT,
                            );
                        }
                    }
                }
            ));
        }
    }

    /// Called after a file was opened into `canvas`. If a backup exists, delete it when it matches the opened
    /// file, otherwise ask whether to restore it.
    pub(crate) async fn offer_backup_restore(&self, canvas: &RnCanvas) {
        let Some(backup) = canvas.backup_file() else {
            return;
        };
        let opened = canvas.output_file().and_then(|f| f.path());
        let found = blocking::unblock({
            let backup = backup.clone();
            move || find_backup(&backup, opened.as_deref())
        })
        .await;
        let (bytes, modified) = match found {
            Ok(FoundBackup::None) => return,
            Ok(FoundBackup::Identical) => {
                remove_backup_file(&backup);
                return;
            }
            Ok(FoundBackup::Differs { bytes, modified }) => (bytes, modified),
            Err(e) => {
                error!("Reading backup file {backup:?} failed, Err: {e:?}");
                return;
            }
        };

        let time = modified
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| gettext("an unknown time"));
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Restore Backup?")),
            Some(&gettext("A backup from {} was found. Restore it?").replace("{}", &time)),
        );
        dialog.add_responses(&[
            ("discard", &gettext("_Discard")),
            ("restore", &gettext("_Restore")),
        ]);
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("restore", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("restore"));
        // Closing the dialog keeps the backup until the document is saved or closed.
        dialog.set_close_response("keep");

        match dialog.choose_future(Some(self)).await.as_str() {
            "restore" => {
                let output_path = canvas.output_file().and_then(|f| f.path());
                match canvas.load_in_rnote_bytes(bytes, output_path).await {
                    Ok(widget_flags) => {
                        self.handle_widget_flags(widget_flags, canvas);
                        // The restored content is not in the real file yet.
                        canvas.set_unsaved_changes(true);
                        canvas.set_backup_pending(false);
                    }
                    Err(e) => {
                        error!("Restoring backup file {backup:?} failed, Err: {e:?}");
                        self.overlays()
                            .dispatch_toast_error(&gettext("Restoring the backup failed"));
                    }
                }
            }
            "discard" => remove_backup_file(&backup),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_path_next_to_source() {
        assert_eq!(
            backup_path(Path::new("/notes/lecture 3.rnote")),
            Some(PathBuf::from("/notes/lecture 3.backup.rnote"))
        );
        assert_eq!(
            backup_path(Path::new("/slides/talk.v2.pdf")),
            Some(PathBuf::from("/slides/talk.v2.backup.rnote"))
        );
        assert_eq!(
            backup_path(Path::new("relative.rnote")),
            Some(PathBuf::from("relative.backup.rnote"))
        );
    }

    #[test]
    fn backup_path_none_for_backups_and_missing_names() {
        assert_eq!(backup_path(Path::new("/notes/a.backup.rnote")), None);
        assert_eq!(backup_path(Path::new("/")), None);
    }

    #[test]
    fn find_backup_cases() {
        let dir = tempfile::tempdir().unwrap();
        let opened = dir.path().join("doc.rnote");
        let backup = backup_path(&opened).unwrap();
        std::fs::write(&opened, b"saved").unwrap();

        assert_eq!(
            find_backup(&backup, Some(&opened)).unwrap(),
            FoundBackup::None
        );

        std::fs::write(&backup, b"saved").unwrap();
        assert_eq!(
            find_backup(&backup, Some(&opened)).unwrap(),
            FoundBackup::Identical
        );

        std::fs::write(&backup, b"newer").unwrap();
        match find_backup(&backup, Some(&opened)).unwrap() {
            FoundBackup::Differs { bytes, modified } => {
                assert_eq!(bytes, b"newer");
                assert!(modified.is_some());
            }
            other => panic!("unexpected {other:?}"),
        }

        // Nothing to compare with (document from a PDF): always offered.
        assert!(matches!(
            find_backup(&backup, None).unwrap(),
            FoundBackup::Differs { .. }
        ));

        remove_backup_file(&backup);
        assert!(!backup.exists());
        // Removing a missing file is fine.
        remove_backup_file(&backup);
    }
}
