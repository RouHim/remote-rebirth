//! The window's state: the project on disk, the library index, the running operation and the
//! page you are looking at.
//!
//! State is grouped by the thing it belongs to so a page never has to reason about the whole
//! window. Handlers live in [`crate::handlers`]; this module owns construction and the lifecycle
//! around an operation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Color, Element, Subscription, Task, Theme};
use remote_rebirth_core::gui_api::{self, DetectReport, WriteResult};
use remote_rebirth_core::model::{ActivityStep, Command, Device, Model};
use remote_rebirth_core::{HidBackend, profile};
use serde::{Deserialize, Serialize};

use crate::message::{
    CommandForm, LibraryDevice, Message, OpDone, Page, Phase, PreparedKind, page_for_digit,
};
use crate::operation::operation_task;
use crate::support::{
    app_data_dir, error_text, form_source, humanize, list_backups, plain_number, scan_library,
    short_digest,
};
use crate::theme;
use crate::views;

/// Names two backups created in the same clock tick apart.
static BACKUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The project on disk and the remote it is being built for.
pub(crate) struct Project {
    pub(crate) model: Model,
    pub(crate) model_path: PathBuf,
    pub(crate) gui_path: PathBuf,
    pub(crate) data_dir: PathBuf,
    pub(crate) container_path: PathBuf,
    pub(crate) backups_dir: PathBuf,
    /// Append-only journal directory; never the user's shared state home in tests.
    pub(crate) state_dir: PathBuf,
    pub(crate) backups: Vec<PathBuf>,
    pub(crate) backup_path: Option<PathBuf>,
    pub(crate) saved: bool,
    /// A text edit is waiting for the debounced autosave tick.
    pub(crate) pending_save: bool,
    pub(crate) model_transfer_path: String,
    pub(crate) remote: Option<DetectReport>,
}

/// The local IR Library browser.
pub(crate) struct LibraryState {
    pub(crate) devices: Vec<LibraryDevice>,
    pub(crate) dir: String,
    pub(crate) search: String,
    pub(crate) selected: Option<usize>,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
}

/// The running remote operation and what it left behind.
pub(crate) struct OperationState {
    pub(crate) phase: Phase,
    pub(crate) busy: bool,
    pub(crate) committing: bool,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) progress: Option<(String, f32)>,
    pub(crate) last_prepare: Option<WriteResult>,
    pub(crate) prepared_kind: Option<PreparedKind>,
    pub(crate) container_size: Option<u64>,
    pub(crate) status: String,
}

/// What the window shows, and the drafts it is holding.
pub(crate) struct UiState {
    pub(crate) page: Page,
    pub(crate) selected_device: usize,
    pub(crate) selected_key: Option<String>,
    pub(crate) command_filter: String,
    pub(crate) device_name_draft: Option<(usize, String)>,
    pub(crate) new_device_name: String,
    pub(crate) new_device_key: u8,
    pub(crate) new_command: CommandForm,
    pub(crate) activity_device: String,
    pub(crate) activity_power: String,
    pub(crate) activity_input: String,
    pub(crate) activity_delay: String,
    pub(crate) new_favourite_channel: String,
    pub(crate) restore_path: String,
    pub(crate) reboot: bool,
    pub(crate) advanced: bool,
    pub(crate) error: Option<String>,
    /// A firmware version warning computed when a Write or Restore is prepared.
    pub(crate) firmware_warning: Option<String>,
}

pub(crate) struct App {
    pub(crate) project: Project,
    pub(crate) library: LibraryState,
    pub(crate) operation: OperationState,
    pub(crate) ui: UiState,
}

/// The window state persisted in `gui.json`.
///
/// `page` stays a `u8` on disk so existing files keep working; [`Page`] owns the mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiState {
    #[serde(default = "default_page")]
    page: u8,
    #[serde(default)]
    selected_device: usize,
    #[serde(default)]
    selected_key: Option<String>,
    #[serde(default = "default_reboot")]
    reboot: bool,
    #[serde(default)]
    advanced: bool,
    #[serde(default)]
    library_dir: Option<String>,
}

fn default_page() -> u8 {
    Page::KeyMap.index()
}

fn default_reboot() -> bool {
    true
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            page: Page::KeyMap.index(),
            selected_device: 0,
            selected_key: None,
            reboot: true,
            advanced: false,
            library_dir: None,
        }
    }
}

fn load_gui_state(path: &Path) -> GuiState {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Read the saved Model, keeping an unreadable file aside before the blank Model can replace it.
fn load_or_preserve_model(model_path: &Path) -> (Model, String, Option<String>) {
    match gui_api::load_model(model_path) {
        Ok(model) => (model, format!("Loaded {}", model_path.display()), None),
        Err(error) => {
            let mut load_error = None;
            if model_path.exists() {
                // The blank Model has no Devices, so the Detect started by `new` adopts the
                // connected skin and persists, which would rename a fresh file over this one.
                // Keep the unreadable file under a clear suffix before that can happen.
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_nanos())
                    .unwrap_or_default();
                let preserved = model_path.with_file_name(format!("model.json.unreadable-{stamp}"));
                load_error = Some(match std::fs::rename(model_path, &preserved) {
                    Ok(()) => format!(
                        "{} cannot be read: {error}. The file was kept as {}.",
                        model_path.display(),
                        preserved.display()
                    ),
                    Err(rename_error) => format!(
                        "{} cannot be read: {error}, and it could not be moved aside: \
                         {rename_error}. Repair or remove it before saving.",
                        model_path.display()
                    ),
                });
            }
            (
                gui_api::blank_model(79),
                "New project. Detect the Remote or add a Device.".to_string(),
                load_error,
            )
        }
    }
}

/// Index the library folder the window last used.
fn scan_library_at_startup(library_dir: &Path) -> (Vec<LibraryDevice>, String, Option<usize>) {
    let devices = scan_library(library_dir).unwrap_or_default();
    let status = if devices.is_empty() {
        "No local IR Library yet. Choose a folder and Scan.".to_string()
    } else {
        String::new()
    };
    let selected = (!devices.is_empty()).then_some(0);
    (devices, status, selected)
}

impl App {
    pub(crate) fn new() -> (Self, Task<Message>) {
        let (mut app, _) = Self::at_with_state(app_data_dir(), remote_rebirth_core::state_dir());
        app.operation.busy = true;
        app.operation.progress = Some(("Detecting".into(), 0.0));
        let task = operation_task(|_| {
            let mut backend = HidBackend::new();
            gui_api::detect(&mut backend)
                .map(OpDone::Detected)
                .map_err(error_text)
        });
        (app, task)
    }

    /// Build the window inside `data_dir`, journaling under `<data_dir>/state`.
    ///
    /// This is the test seam: a test never reaches the shared state home.
    #[cfg(test)]
    pub(crate) fn at(data_dir: PathBuf) -> (Self, Task<Message>) {
        let state_dir = data_dir.join("state");
        Self::at_with_state(data_dir, state_dir)
    }

    fn at_with_state(data_dir: PathBuf, state_dir: PathBuf) -> (Self, Task<Message>) {
        let _ = std::fs::create_dir_all(&data_dir);
        let backups_dir = data_dir.join("backups");
        let _ = std::fs::create_dir_all(&backups_dir);
        let _ = std::fs::create_dir_all(&state_dir);
        let container_path = data_dir.join("container.bin");
        let model_path = data_dir.join("model.json");
        let gui_path = data_dir.join("gui.json");
        let gui_state = load_gui_state(&gui_path);
        let (model, status, load_error) = load_or_preserve_model(&model_path);
        let backups = list_backups(&backups_dir);
        let library_dir = gui_state
            .library_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("ir-library"));
        let (devices, library_status, library_selected) = scan_library_at_startup(&library_dir);
        let selected_device = gui_state
            .selected_device
            .min(model.devices.len().saturating_sub(1));
        let activity_device = model
            .devices
            .first()
            .map(|device| device.name.clone())
            .unwrap_or_default();
        let page = Page::from_index(gui_state.page);
        let page = if model.devices.is_empty() {
            Page::Remote
        } else {
            page
        };
        (
            Self {
                project: Project {
                    model,
                    model_path: model_path.clone(),
                    gui_path,
                    data_dir,
                    container_path,
                    backups_dir,
                    state_dir,
                    backups,
                    backup_path: None,
                    saved: true,
                    pending_save: false,
                    model_transfer_path: model_path.display().to_string(),
                    remote: None,
                },
                library: LibraryState {
                    devices,
                    dir: library_dir.display().to_string(),
                    search: String::new(),
                    selected: library_selected,
                    status: library_status,
                    error: None,
                },
                operation: OperationState {
                    phase: Phase::Detect,
                    busy: false,
                    committing: false,
                    cancel: Arc::new(AtomicBool::new(false)),
                    progress: None,
                    last_prepare: None,
                    prepared_kind: None,
                    container_size: None,
                    status,
                },
                ui: UiState {
                    firmware_warning: None,
                    page,
                    selected_device,
                    selected_key: gui_state.selected_key,
                    command_filter: String::new(),
                    device_name_draft: None,
                    new_device_name: String::new(),
                    new_device_key: 1,
                    new_command: CommandForm::default(),
                    activity_device,
                    activity_power: String::new(),
                    activity_input: String::new(),
                    activity_delay: "500".into(),
                    new_favourite_channel: String::new(),
                    restore_path: String::new(),
                    reboot: gui_state.reboot,
                    advanced: gui_state.advanced,
                    error: load_error,
                },
            },
            Task::none(),
        )
    }

    pub(crate) fn theme(&self) -> Theme {
        Theme::custom(
            "Remote Rebirth",
            iced::theme::Palette {
                background: theme::PAPER,
                text: theme::INK,
                primary: theme::OXIDE,
                success: theme::GRASS,
                warning: Color::from_rgb8(0xA8, 0x62, 0x2A),
                danger: Color::from_rgb8(0x8E, 0x3B, 0x32),
            },
        )
    }

    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let keys = iced::keyboard::listen().filter_map(|event| match event {
            iced::keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Some(Message::Shortcut(key, modifiers))
            }
            _ => None,
        });
        if self.project.pending_save {
            iced::Subscription::batch([
                keys,
                iced::time::every(Duration::from_millis(400)).map(|_instant| Message::Autosave),
            ])
        } else {
            keys
        }
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        let operation_message = matches!(
            &message,
            Message::Progress(_) | Message::Done(_) | Message::Cancel
        );
        if self.operation.busy && !operation_message {
            return Task::none();
        }
        match message {
            Message::Shortcut(key, modifiers) => self.shortcut(key, modifiers),
            Message::Page(_)
            | Message::UseDetectedSkin(_)
            | Message::Reboot(_)
            | Message::RestorePath(_)
            | Message::UseBackup(_)
            | Message::RefreshBackups
            | Message::DeleteBackup(_)
            | Message::DismissError
            | Message::DismissFirmwareWarning => self.on_remote(message),
            Message::Autosave => {
                if self.project.pending_save {
                    self.persist();
                }
                Task::none()
            }
            Message::Detect
            | Message::Backup
            | Message::Compose
            | Message::PrepareWrite
            | Message::CommitWrite
            | Message::PrepareRestore
            | Message::CommitRestore
            | Message::Progress(_)
            | Message::Done(_)
            | Message::Cancel => self.on_operation(message),
            Message::SelectDevice(_)
            | Message::NewDeviceName(_)
            | Message::NewDeviceKey(_)
            | Message::AddDevice
            | Message::DeviceName(..)
            | Message::DeviceKey(..)
            | Message::DeleteDevice(_)
            | Message::NewCommandName(_)
            | Message::NewCommandFormat(_)
            | Message::NewCommandFile(_)
            | Message::NewCommandRemote(_)
            | Message::NewCommandButton(_)
            | Message::NewCommandHex(_)
            | Message::AddCommand
            | Message::DeleteCommand(_) => self.on_devices(message),
            Message::SelectKey(_)
            | Message::ClearKey
            | Message::CommandFilter(_)
            | Message::AssignCommand(_)
            | Message::AssignFirstCommand
            | Message::RoutingVolume(_)
            | Message::RoutingChannel(_) => self.on_keymap(message),
            Message::ActivityChannel(_)
            | Message::ActivityDevice(_)
            | Message::ActivityPower(_)
            | Message::ActivityInput(_)
            | Message::ActivityDelay(_)
            | Message::StepDevice(..)
            | Message::StepPower(..)
            | Message::StepInput(..)
            | Message::StepDelay(..)
            | Message::AddStep
            | Message::DeleteStep(_) => self.on_activities(message),
            Message::NewFavouriteChannel(_)
            | Message::AddFavourite
            | Message::FavouriteChannel(..)
            | Message::DeleteFavourite(_) => self.on_favourites(message),
            Message::LibraryPath(_)
            | Message::ScanLibrary
            | Message::LibrarySearch(_)
            | Message::SelectLibraryDevice(_)
            | Message::AddLibraryDevice => self.on_library(message),
            Message::SkinChanged(_)
            | Message::Save
            | Message::Reload
            | Message::TransferPath(_)
            | Message::ExportModel
            | Message::ImportModel
            | Message::ToggleAdvanced(_) => self.on_settings(message),
        }
    }

    pub(crate) fn shortcut(&mut self, key: Key, modifiers: Modifiers) -> Task<Message> {
        let key = key.as_ref();

        if modifiers.control() && modifiers.alt() {
            match key {
                Key::Named(Named::ArrowLeft) => self.select_relative_device(-1),
                Key::Named(Named::ArrowRight) => self.select_relative_device(1),
                Key::Named(Named::ArrowUp) => self.select_relative_key(-1),
                Key::Named(Named::ArrowDown) => self.select_relative_key(1),
                Key::Named(Named::Backspace) => {
                    self.ui.selected_key = None;
                    self.save_gui_state();
                }
                _ => {}
            }
            return Task::none();
        }

        if modifiers.control() {
            if let Key::Character(text) = key
                && let Some(page) = page_for_digit(text)
            {
                self.set_page(page);
                return Task::none();
            }
            match key {
                Key::Character("b") | Key::Character("B") => {
                    return self.update(Message::Backup);
                }
                Key::Character("r") | Key::Character("R") => {
                    if self.validation().is_none() {
                        return self.update(Message::PrepareWrite);
                    }
                }
                Key::Character("f") | Key::Character("F") => {
                    return self.update(Message::Detect);
                }
                Key::Named(Named::Enter) => {
                    if self.ui.page == Page::KeyMap {
                        return self.assign_first_command();
                    }
                }
                Key::Named(Named::PageUp) => self.cycle_page(-1),
                Key::Named(Named::PageDown) => self.cycle_page(1),
                _ => {}
            }
            return Task::none();
        }

        if matches!(key, Key::Named(Named::Escape))
            && self.ui.error.take().is_none()
            && self.ui.selected_key.take().is_some()
        {
            self.save_gui_state();
        }
        Task::none()
    }

    pub(crate) fn set_page(&mut self, page: Page) {
        self.ui.page = page;
        self.save_gui_state();
    }

    pub(crate) fn cycle_page(&mut self, delta: isize) {
        self.set_page(self.ui.page.cycle(delta));
    }

    pub(crate) fn select_relative_device(&mut self, delta: isize) {
        if self.project.model.devices.is_empty() {
            return;
        }
        let len = self.project.model.devices.len() as isize;
        let next = (self.ui.selected_device as isize + delta).rem_euclid(len) as usize;
        self.ui.selected_device = next;
        self.ui.selected_key = None;
        self.sync_device_name_draft();
        self.save_gui_state();
    }

    pub(crate) fn select_relative_key(&mut self, delta: isize) {
        let keys: Vec<&str> = profile::HARMONY_300
            .key_layout
            .iter()
            .map(|binding| binding.name)
            .collect();
        if keys.is_empty() {
            return;
        }
        let current = self
            .ui
            .selected_key
            .as_deref()
            .and_then(|selected| keys.iter().position(|key| *key == selected))
            .unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(keys.len() as isize) as usize;
        self.ui.selected_key = Some(keys[next].to_string());
        self.save_gui_state();
    }

    pub(crate) fn assign_first_command(&mut self) -> Task<Message> {
        if self.ui.selected_key.is_none() {
            return Task::none();
        }
        let filter = self.ui.command_filter.to_lowercase();
        let command = self.selected_device().and_then(|device| {
            device
                .commands
                .keys()
                .find(|name| filter.is_empty() || name.to_lowercase().contains(&filter))
                .cloned()
        });
        match command {
            Some(command) => self.update(Message::AssignCommand(command)),
            None => Task::none(),
        }
    }

    pub(crate) fn begin(&mut self, status: &str) {
        self.operation.busy = true;
        self.operation.committing = false;
        self.operation.cancel.store(false, Ordering::Relaxed);
        self.ui.error = None;
        self.ui.firmware_warning = None;
        self.operation.progress = Some((status.to_string(), 0.0));
        self.operation.status = status.to_string();
        self.operation.phase = match status {
            "Detecting the Remote" => Phase::Detect,
            "Backing up the Remote" => Phase::Backup,
            "Composing" | "Preparing a Write" | "Preparing a Restore" => Phase::Prepare,
            "Writing to the Remote" | "Restoring the Remote" => Phase::Commit,
            _ => self.operation.phase,
        };
    }

    pub(crate) fn finish(&mut self, done: OpDone) {
        match done {
            OpDone::Detected(report) => {
                self.operation.prepared_kind = None;
                self.operation.last_prepare = None;
                if self.project.model.devices.is_empty()
                    && report.supported
                    && let Some(skin) = report.skin
                    && profile::skin_profile(&profile::HARMONY_300, skin).is_some()
                {
                    self.project.model.skin = skin;
                    self.persist();
                }
                self.operation.status = if report.supported {
                    if self.ui.advanced {
                        format!(
                            "{} connected — Skin {}, firmware {}",
                            report.model,
                            plain_number(report.skin),
                            report.firmware_version.as_deref().unwrap_or("unknown")
                        )
                    } else {
                        format!("{} connected", report.model)
                    }
                } else {
                    format!("{} connected — not supported", report.model)
                };
                self.operation.phase = if report.supported {
                    if self.project.backups.is_empty() {
                        Phase::Backup
                    } else {
                        Phase::Prepare
                    }
                } else {
                    Phase::Detect
                };
                self.project.remote = Some(report);
            }
            OpDone::BackedUp(report) => {
                self.project.backup_path = Some(report.container.clone());
                self.operation.prepared_kind = None;
                self.operation.last_prepare = None;
                self.operation.phase = Phase::Prepare;
                self.refresh_backups();
                self.operation.status = if self.ui.advanced {
                    format!(
                        "Backup saved — {} bytes, validation {}, sha256 {}",
                        report.size,
                        report.validation.as_str(),
                        report.sha256
                    )
                } else {
                    "Backup saved.".into()
                };
            }
            OpDone::Composed(report) => {
                self.operation.container_size = Some(report.size);
                self.operation.prepared_kind = None;
                self.operation.last_prepare = None;
                self.operation.phase = Phase::Prepare;
                self.operation.status = if self.ui.advanced {
                    let mut status = format!(
                        "Setup checked — {} bytes, sha256 {}, {} device(s)",
                        report.size, report.sha256, report.devices
                    );
                    if !report.unused.is_empty() {
                        status
                            .push_str(&format!("; {} unused command(s) kept", report.unused.len()));
                    }
                    status
                } else {
                    "Setup checked. Ready to update the remote.".into()
                };
            }
            OpDone::Prepared(report) => {
                self.project.backup_path = Some(report.backup.clone());
                self.operation.container_size = Some(report.bytes as u64);
                self.refresh_backups();
                self.operation.last_prepare = Some(report.clone());
                self.operation.prepared_kind = Some(PreparedKind::Write);
                self.operation.phase = Phase::Commit;
                self.ui.page = Page::Remote;
                self.operation.status = if self.ui.advanced {
                    format!(
                        "Ready to update — {} bytes, {} chunk(s), {} data packet(s); backup {}",
                        report.bytes,
                        report.chunks,
                        report.data_packets,
                        short_digest(&report.backup_sha256)
                    )
                } else {
                    "Ready to update the remote. A backup was saved first.".into()
                };
            }
            OpDone::Committed(report) => {
                self.operation.last_prepare = None;
                self.operation.prepared_kind = None;
                self.operation.phase = Phase::ReadBack;
                self.operation.container_size = Some(report.bytes as u64);
                self.operation.status = if self.ui.advanced {
                    format!(
                        "Remote updated — {} bytes, verified, reboot {}",
                        report.bytes,
                        if report.reboot {
                            "requested"
                        } else {
                            "suppressed"
                        }
                    )
                } else {
                    "Remote updated successfully.".into()
                };
            }
            OpDone::RestorePrepared(report) => {
                self.operation.last_prepare = Some(report.clone());
                self.operation.prepared_kind = Some(PreparedKind::Restore);
                self.operation.phase = Phase::Commit;
                self.operation.status = if self.ui.advanced {
                    format!(
                        "Ready to restore — {} bytes, {} chunk(s)",
                        report.bytes, report.chunks
                    )
                } else {
                    "Ready to restore the backup.".into()
                };
            }
            OpDone::Restored(report) => {
                self.operation.last_prepare = None;
                self.operation.prepared_kind = None;
                self.operation.phase = Phase::ReadBack;
                self.operation.container_size = Some(report.bytes as u64);
                self.operation.status = if self.ui.advanced {
                    format!(
                        "Backup restored — {} bytes, verified, reboot {}",
                        report.bytes,
                        if report.reboot {
                            "requested"
                        } else {
                            "suppressed"
                        }
                    )
                } else {
                    "Backup restored successfully.".into()
                };
            }
        }
    }

    /// Mark the Model dirty so the debounced autosave tick persists it.
    ///
    /// The prepared operation is dropped here, not on the tick: a Write or Restore prepared
    /// against the Model on disk stops being trustworthy the moment the Model in memory changes,
    /// and the debounce would otherwise leave it committable for one more window.
    pub(crate) fn mark_dirty(&mut self) {
        self.project.pending_save = true;
        self.project.saved = false;
        self.operation.prepared_kind = None;
        self.operation.last_prepare = None;
        self.ui.firmware_warning = None;
    }

    /// Compute the firmware warning for the next Prepare, from the last Detect report.
    pub(crate) fn refresh_firmware_warning(&mut self) {
        self.ui.firmware_warning = self.project.remote.as_ref().and_then(|remote| {
            let profile = profile::by_id(&self.project.model.profile)?;
            if profile::firmware_matches(profile, remote.firmware_version.as_deref()) {
                None
            } else {
                Some(format!(
                    "This remote reports firmware {}. The write sequence is verified on {} only, so this update is unverified.",
                    remote.firmware_version.as_deref().unwrap_or("unknown"),
                    profile.firmware
                ))
            }
        });
    }

    pub(crate) fn save_gui_state(&mut self) {
        let state = GuiState {
            page: self.ui.page.index(),
            selected_device: self.ui.selected_device,
            selected_key: self.ui.selected_key.clone(),
            reboot: self.ui.reboot,
            advanced: self.ui.advanced,
            library_dir: Some(self.library.dir.clone()),
        };
        let written = serde_json::to_vec_pretty(&state)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                gui_api::write_atomic(&self.project.gui_path, &bytes)
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = written {
            self.ui.error = Some(format!("Could not save the window state: {error}"));
        }
    }

    pub(crate) fn persist(&mut self) {
        self.project.pending_save = false;
        self.save_gui_state();
        // A prepared Write or Restore is only trustworthy while `model.json` matches the Model in
        // memory, so a failed save must drop it as well as a successful one.
        self.operation.prepared_kind = None;
        self.operation.last_prepare = None;
        self.ui.firmware_warning = None;
        if let Err(error) = gui_api::save_model(&self.project.model_path, &self.project.model) {
            self.ui.error = Some(error_text(error));
            self.project.saved = false;
        } else {
            self.project.saved = true;
        }
    }

    pub(crate) fn refresh_backups(&mut self) {
        self.project.backups = list_backups(&self.project.backups_dir);
    }

    /// A fresh backup name. The clock makes it readable; the counter keeps two operations in the
    /// same clock tick from choosing the same file.
    pub(crate) fn backup_path_for(&self, prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let sequence = BACKUP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        self.project
            .backups_dir
            .join(format!("{prefix}-{stamp}-{sequence}.bin"))
    }

    pub(crate) fn restore_path(&self) -> Result<PathBuf, String> {
        let value = self.ui.restore_path.trim();
        if value.is_empty() {
            return Err("Choose a Backup path to restore.".into());
        }
        let path = PathBuf::from(value);
        if !path.exists() {
            return Err(format!("Backup {} does not exist.", path.display()));
        }
        Ok(path)
    }

    pub(crate) fn selected_device(&self) -> Option<&Device> {
        self.project.model.devices.get(self.ui.selected_device)
    }

    pub(crate) fn device_names(&self) -> Vec<String> {
        self.project
            .model
            .devices
            .iter()
            .map(|device| device.name.clone())
            .collect()
    }

    /// Keep the in-progress Device name text of the selected row in step with the Model,
    /// so a rejected rename never leaks into another Device or a reloaded Model.
    pub(crate) fn sync_device_name_draft(&mut self) {
        let draft = self
            .project
            .model
            .devices
            .get(self.ui.selected_device)
            .map(|device| (self.ui.selected_device, device.name.clone()));
        self.ui.device_name_draft = draft;
    }

    pub(crate) fn validation(&self) -> Option<String> {
        gui_api::validate_model(&self.project.model)
            .err()
            .map(error_text)
    }

    pub(crate) fn add_device(&mut self) {
        let name = self.ui.new_device_name.trim().to_string();
        if name.is_empty() {
            self.ui.error = Some("Give the device a name.".into());
            return;
        }
        if self
            .project
            .model
            .devices
            .iter()
            .any(|device| device.name == name)
        {
            self.ui.error = Some(format!("Device '{name}' already exists."));
            return;
        }
        if self
            .project
            .model
            .devices
            .iter()
            .any(|device| device.device_key == self.ui.new_device_key)
        {
            self.ui.error = Some(format!(
                "Device Key {} is already assigned.",
                self.ui.new_device_key
            ));
            return;
        }
        self.project.model.devices.push(Device {
            name: name.clone(),
            device_key: self.ui.new_device_key,
            commands: BTreeMap::new(),
            keys: BTreeMap::new(),
        });
        if self.project.model.activity.channel_device.is_empty() {
            self.project.model.activity.channel_device = name.clone();
        }
        if self.project.model.activity.steps.is_empty() {
            self.project.model.activity.steps.push(ActivityStep {
                device: name.clone(),
                power: Some("power".into()),
                input: None,
                delay_ms: 500,
            });
        }
        self.ui.selected_device = self.project.model.devices.len() - 1;
        self.ui.activity_device = name.clone();
        self.ui.new_device_name.clear();
        self.sync_device_name_draft();
        self.persist();
        self.operation.status = format!("Added Device '{name}'.");
    }

    pub(crate) fn add_command(&mut self) {
        if self.selected_device().is_none() {
            self.ui.error = Some("Select or add a Device first.".into());
            return;
        }
        let form = self.ui.new_command.clone();
        let name = form.name.trim().to_string();
        if name.is_empty() {
            self.ui.error = Some("Give the command a name.".into());
            return;
        }
        let source = match form_source(&form) {
            Ok(source) => source,
            Err(error) => {
                self.ui.error = Some(error);
                return;
            }
        };
        if let Some(device) = self.project.model.devices.get_mut(self.ui.selected_device) {
            device.commands.insert(
                name.clone(),
                Command {
                    source,
                    sends: None,
                },
            );
            let device_name = device.name.clone();
            for step in &mut self.project.model.activity.steps {
                if step.device == device_name {
                    if step
                        .power
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case(&name))
                    {
                        step.power = Some(name.clone());
                    }
                    if step
                        .input
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case(&name))
                    {
                        step.input = Some(name.clone());
                    }
                    if step.power.is_none() && name.eq_ignore_ascii_case("power") {
                        step.power = Some(name.clone());
                    }
                    if step.input.is_none() && name.eq_ignore_ascii_case("input") {
                        step.input = Some(name.clone());
                    }
                }
            }
            self.ui.new_command.name.clear();
            self.persist();
            self.operation.status =
                format!("Added command '{}' to {device_name}.", humanize(&name));
        }
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
        views::view(self)
    }
}
