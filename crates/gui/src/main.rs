//! Remote Rebirth desktop GUI.
//!
//! The first slice is deliberately direct: Detect, Back up, build a one-Device Model, edit its Key
//! Map with volume/channel Routing, Compose, Prepare, Commit and Restore. The IR Library browser,
//! Favourites polish and Activity polish follow in the next slice.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use iced::futures::StreamExt;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::task::{Straw, sipper};
use iced::{Color, Element, Size, Subscription, Task, Theme};
use remote_rebirth_core::gui_api::{self, BackupResult, DetectReport, WriteResult};
use remote_rebirth_core::model::{
    ActivityStep, Command, Device, Favourite, IrSource, Model, Routing, SCHEMA_VERSION,
};
use remote_rebirth_core::{HidBackend, Progress, SystemClock, profile};
use serde::{Deserialize, Serialize};

mod theme;
mod views;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Remote Rebirth")
        .theme(App::theme)
        .subscription(App::subscription)
        .font(include_bytes!("../assets/fonts/PublicSans-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/PublicSans-Medium.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/PublicSans-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/PublicSans-Bold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/SourceSerif4-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/SplineSansMono-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/SplineSansMono-Medium.ttf").as_slice())
        .window(iced::window::Settings {
            size: Size::new(1240.0, 820.0),
            min_size: Some(Size::new(1040.0, 680.0)),
            position: iced::window::Position::Centered,
            ..Default::default()
        })
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreparedKind {
    Write,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Detect,
    Backup,
    Prepare,
    Commit,
    ReadBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Remote,
    Devices,
    KeyMap,
    Activities,
    Favourites,
    Library,
    Settings,
}

#[derive(Debug, Clone)]
enum OpDone {
    Detected(DetectReport),
    BackedUp(BackupResult),
    Composed(remote_rebirth_core::gui_api::ComposeResult),
    Prepared(WriteResult),
    Committed(WriteResult),
    RestorePrepared(WriteResult),
    Restored(WriteResult),
}

#[derive(Debug, Clone)]
enum Message {
    Page(Page),
    Shortcut(Key, Modifiers),
    Detect,
    Backup,
    Compose,
    PrepareWrite,
    CommitWrite,
    PrepareRestore,
    CommitRestore,
    Progress(Progress),
    Done(Result<OpDone, String>),
    Cancel,
    SelectDevice(usize),
    NewDeviceName(String),
    NewDeviceKey(u8),
    AddDevice,
    DeviceName(usize, String),
    DeviceKey(usize, u8),
    DeleteDevice(usize),
    SelectKey(String),
    ClearKey,
    CommandFilter(String),
    AssignCommand(String),
    AssignFirstCommand,
    NewCommandName(String),
    NewCommandFormat(String),
    NewCommandFile(String),
    NewCommandRemote(String),
    NewCommandButton(String),
    NewCommandHex(String),
    AddCommand,
    DeleteCommand(String),
    RoutingVolume(Option<String>),
    RoutingChannel(Option<String>),
    SkinChanged(u8),
    UseDetectedSkin(u8),
    Reboot(bool),
    ToggleAdvanced(bool),
    ActivityChannel(String),
    ActivityDevice(String),
    ActivityPower(String),
    ActivityInput(String),
    ActivityDelay(String),
    StepDevice(usize, String),
    StepPower(usize, String),
    StepInput(usize, String),
    StepDelay(usize, String),
    AddStep,
    DeleteStep(usize),
    NewFavouriteChannel(String),
    AddFavourite,
    FavouriteChannel(usize, String),
    DeleteFavourite(usize),
    LibraryPath(String),
    ScanLibrary,
    LibrarySearch(String),
    SelectLibraryDevice(usize),
    AddLibraryDevice,
    RestorePath(String),
    Save,
    Reload,
    TransferPath(String),
    ExportModel,
    ImportModel,
    UseBackup(PathBuf),
    RefreshBackups,
    DeleteBackup(PathBuf),
    DismissError,
}

#[derive(Debug, Clone)]
struct LibraryDevice {
    brand: String,
    model: String,
    path: PathBuf,
    commands: Vec<String>,
}

#[derive(Debug, Clone)]
struct CommandForm {
    name: String,
    format: String,
    file: String,
    remote: String,
    button: String,
    hex: String,
}

impl Default for CommandForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            format: "pronto".into(),
            file: String::new(),
            remote: String::new(),
            button: String::new(),
            hex: String::new(),
        }
    }
}

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
    2
}

fn default_reboot() -> bool {
    true
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            page: 2,
            selected_device: 0,
            selected_key: None,
            reboot: true,
            advanced: false,
            library_dir: None,
        }
    }
}

struct App {
    page: Page,
    model: Model,
    model_path: PathBuf,
    gui_path: PathBuf,
    data_dir: PathBuf,
    container_path: PathBuf,
    backups_dir: PathBuf,
    remote: Option<DetectReport>,
    selected_device: usize,
    selected_key: Option<String>,
    command_filter: String,
    device_name_draft: Option<(usize, String)>,
    new_device_name: String,
    new_device_key: u8,
    new_command: CommandForm,
    activity_device: String,
    activity_power: String,
    activity_input: String,
    activity_delay: String,
    new_favourite_channel: String,
    library_dir: String,
    library: Vec<LibraryDevice>,
    library_search: String,
    library_selected: Option<usize>,
    library_status: String,
    library_error: Option<String>,
    backup_path: Option<PathBuf>,
    backups: Vec<PathBuf>,
    container_size: Option<u64>,
    saved: bool,
    reboot: bool,
    advanced: bool,
    restore_path: String,
    model_transfer_path: String,
    status: String,
    error: Option<String>,
    busy: bool,
    committing: bool,
    cancel: Arc<AtomicBool>,
    progress: Option<(String, f32)>,
    last_prepare: Option<WriteResult>,
    prepared_kind: Option<PreparedKind>,
    phase: Phase,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let (mut app, _) = Self::at(app_data_dir());
        app.busy = true;
        app.progress = Some(("Detecting".into(), 0.0));
        let task = operation_task(|_| {
            let mut backend = HidBackend::new();
            gui_api::detect(&mut backend)
                .map(OpDone::Detected)
                .map_err(error_text)
        });
        (app, task)
    }

    fn at(data_dir: PathBuf) -> (Self, Task<Message>) {
        let _ = std::fs::create_dir_all(&data_dir);
        let backups_dir = data_dir.join("backups");
        let _ = std::fs::create_dir_all(&backups_dir);
        let model_path = data_dir.join("model.json");
        let gui_path = data_dir.join("gui.json");
        let gui_state: GuiState = std::fs::read(&gui_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let mut status = "New project. Detect the Remote or add a Device.".to_string();
        let mut load_error = None;
        let model = match gui_api::load_model(&model_path) {
            Ok(model) => {
                status = format!("Loaded {}", model_path.display());
                model
            }
            Err(error) => {
                if model_path.exists() {
                    // The blank Model has no Devices, so the Detect started by `new` adopts the
                    // connected skin and persists, which would rename a fresh file over this one.
                    // Keep the unreadable file under a clear suffix before that can happen.
                    let stamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or_default();
                    let preserved =
                        model_path.with_file_name(format!("model.json.unreadable-{stamp}"));
                    load_error = Some(match std::fs::rename(&model_path, &preserved) {
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
                gui_api::blank_model(79)
            }
        };
        let backups = list_backups(&backups_dir);
        let library_dir = gui_state
            .library_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("ir-library"));
        let library = scan_library(&library_dir).unwrap_or_default();
        let library_status = if library.is_empty() {
            "No local IR Library yet. Choose a folder and Scan.".to_string()
        } else {
            String::new()
        };
        let library_selected = (!library.is_empty()).then_some(0);
        let selected_device = gui_state
            .selected_device
            .min(model.devices.len().saturating_sub(1));
        let selected_key = gui_state.selected_key;
        let activity_device = model
            .devices
            .first()
            .map(|device| device.name.clone())
            .unwrap_or_default();
        let page = match gui_state.page {
            1 => Page::Devices,
            2 => Page::KeyMap,
            3 => Page::Activities,
            4 => Page::Favourites,
            5 => Page::Library,
            6 => Page::Settings,
            _ => Page::Remote,
        };
        let page = if model.devices.is_empty() {
            Page::Remote
        } else {
            page
        };
        (
            Self {
                page,
                model,
                model_path: model_path.clone(),
                gui_path,
                data_dir: data_dir.clone(),
                container_path: data_dir.join("container.bin"),
                backups_dir,
                remote: None,
                selected_device,
                selected_key,
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
                library_dir: library_dir.display().to_string(),
                library,
                library_search: String::new(),
                library_selected,
                library_status,
                library_error: None,
                backup_path: None,
                backups,
                container_size: None,
                saved: true,
                reboot: gui_state.reboot,
                advanced: gui_state.advanced,
                restore_path: String::new(),
                model_transfer_path: model_path.display().to_string(),
                status,
                error: load_error,
                busy: false,
                committing: false,
                cancel: Arc::new(AtomicBool::new(false)),
                progress: None,
                last_prepare: None,
                prepared_kind: None,
                phase: Phase::Detect,
            },
            Task::none(),
        )
    }

    fn theme(&self) -> Theme {
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

    fn subscription(&self) -> Subscription<Message> {
        iced::keyboard::listen().filter_map(|event| match event {
            iced::keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Some(Message::Shortcut(key, modifiers))
            }
            _ => None,
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let operation_message = matches!(
            &message,
            Message::Progress(_) | Message::Done(_) | Message::Cancel
        );
        if self.busy && !operation_message {
            return Task::none();
        }
        match message {
            Message::Shortcut(key, modifiers) => self.shortcut(key, modifiers),
            Message::Page(page) => {
                self.page = page;
                self.save_gui_state();
                Task::none()
            }
            Message::Detect => {
                self.begin("Detecting the Remote");
                let cancel = self.cancel.clone();
                operation_task(move |_| {
                    check_cancel(&cancel)?;
                    let mut backend = HidBackend::new();
                    gui_api::detect(&mut backend)
                        .map(OpDone::Detected)
                        .map_err(error_text)
                })
            }
            Message::Backup => {
                let path = self.backup_path_for("backup");
                let cancel = self.cancel.clone();
                self.begin("Backing up the Remote");
                operation_task(move |_| {
                    check_cancel(&cancel)?;
                    let mut backend = HidBackend::new();
                    let clock = SystemClock;
                    gui_api::backup(&mut backend, &clock, &path, false)
                        .map(OpDone::BackedUp)
                        .map_err(error_text)
                })
            }
            Message::Compose => {
                let model = self.model.clone();
                let model_path = self.model_path.clone();
                let container = self.container_path.clone();
                let cancel = self.cancel.clone();
                self.begin("Composing");
                operation_task(move |progress| {
                    check_cancel(&cancel)?;
                    gui_api::save_model(&model_path, &model).map_err(error_text)?;
                    check_cancel(&cancel)?;
                    let clock = SystemClock;
                    progress(Progress::Step("compose"));
                    gui_api::compose(&clock, &model_path, &container, true)
                        .map(OpDone::Composed)
                        .map_err(error_text)
                })
            }
            Message::PrepareWrite => {
                let model = self.model.clone();
                let model_path = self.model_path.clone();
                let container = self.container_path.clone();
                let backup = self.backup_path_for("pre-write");
                let reboot = self.reboot;
                let cancel = self.cancel.clone();
                self.begin("Preparing a Write");
                operation_task(move |progress| {
                    check_cancel(&cancel)?;
                    gui_api::save_model(&model_path, &model).map_err(error_text)?;
                    check_cancel(&cancel)?;
                    let clock = SystemClock;
                    progress(Progress::Step("compose"));
                    gui_api::compose(&clock, &model_path, &container, true).map_err(error_text)?;
                    check_cancel(&cancel)?;
                    progress(Progress::Step("backup"));
                    let mut backend = HidBackend::new();
                    gui_api::backup(&mut backend, &clock, &backup, true).map_err(error_text)?;
                    check_cancel(&cancel)?;
                    let report = gui_api::write(
                        &mut backend,
                        &clock,
                        &remote_rebirth_core::state_dir(),
                        &container,
                        &backup,
                        false,
                        !reboot,
                        progress,
                    )
                    .map_err(error_text)?;
                    Ok(OpDone::Prepared(report))
                })
            }
            Message::CommitWrite if self.prepared_kind == Some(PreparedKind::Write) => {
                let reboot = self.reboot;
                match (self.container_path.clone(), self.backup_path.clone()) {
                    (container, Some(backup)) => {
                        self.begin("Writing to the Remote");
                        self.committing = true;
                        operation_task(move |progress| {
                            let mut backend = HidBackend::new();
                            let clock = SystemClock;
                            gui_api::write(
                                &mut backend,
                                &clock,
                                &remote_rebirth_core::state_dir(),
                                &container,
                                &backup,
                                true,
                                !reboot,
                                progress,
                            )
                            .map(OpDone::Committed)
                            .map_err(error_text)
                        })
                    }
                    _ => {
                        self.error = Some("Prepare a Write first.".into());
                        Task::none()
                    }
                }
            }
            Message::CommitWrite => {
                self.error = Some("Prepare a Write first.".into());
                Task::none()
            }
            Message::PrepareRestore => {
                let path = self.restore_path();
                let reboot = self.reboot;
                let cancel = self.cancel.clone();
                match path {
                    Ok(path) => {
                        self.begin("Preparing a Restore");
                        operation_task(move |progress| {
                            check_cancel(&cancel)?;
                            let mut backend = HidBackend::new();
                            let clock = SystemClock;
                            gui_api::restore(
                                &mut backend,
                                &clock,
                                &remote_rebirth_core::state_dir(),
                                &path,
                                false,
                                !reboot,
                                progress,
                            )
                            .map(OpDone::RestorePrepared)
                            .map_err(error_text)
                        })
                    }
                    Err(error) => {
                        self.error = Some(error);
                        Task::none()
                    }
                }
            }
            Message::CommitRestore if self.prepared_kind == Some(PreparedKind::Restore) => {
                let path = self.restore_path();
                let reboot = self.reboot;
                let cancel = self.cancel.clone();
                match path {
                    Ok(path) => {
                        self.begin("Restoring the Remote");
                        self.committing = true;
                        operation_task(move |progress| {
                            check_cancel(&cancel)?;
                            let mut backend = HidBackend::new();
                            let clock = SystemClock;
                            gui_api::restore(
                                &mut backend,
                                &clock,
                                &remote_rebirth_core::state_dir(),
                                &path,
                                true,
                                !reboot,
                                progress,
                            )
                            .map(OpDone::Restored)
                            .map_err(error_text)
                        })
                    }
                    Err(error) => {
                        self.error = Some(error);
                        Task::none()
                    }
                }
            }
            Message::CommitRestore => {
                self.error = Some("Prepare a Restore first.".into());
                Task::none()
            }
            Message::Progress(progress) => {
                match progress {
                    Progress::Step(step) => {
                        self.progress = Some((step_label(step).to_string(), 0.0));
                    }
                    Progress::Written { bytes, total } => {
                        let fraction = if total == 0 {
                            0.0
                        } else {
                            bytes as f32 / total as f32
                        };
                        self.progress = Some((format!("writing {bytes}/{total}"), fraction));
                    }
                }
                Task::none()
            }
            Message::Done(result) => {
                self.busy = false;
                self.committing = false;
                self.progress = None;
                match result {
                    Ok(done) => self.finish(done),
                    Err(error) => {
                        self.error = Some(error);
                        self.status = "Operation failed.".into();
                    }
                }
                Task::none()
            }
            Message::SelectDevice(index) => {
                if index < self.model.devices.len() {
                    self.selected_device = index;
                    self.selected_key = None;
                    self.activity_device = self.model.devices[index].name.clone();
                    self.sync_device_name_draft();
                    self.save_gui_state();
                }
                Task::none()
            }
            Message::NewDeviceName(name) => {
                self.new_device_name = name;
                Task::none()
            }
            Message::NewDeviceKey(key) => {
                self.new_device_key = key;
                Task::none()
            }
            Message::AddDevice => {
                self.add_device();
                Task::none()
            }
            Message::DeviceName(index, name) => {
                if self.model.devices.get(index).is_none() {
                    return Task::none();
                }
                self.device_name_draft = Some((index, name.clone()));
                let duplicate = self
                    .model
                    .devices
                    .iter()
                    .enumerate()
                    .any(|(other, device)| other != index && device.name == name);
                if duplicate {
                    self.error = Some(format!("Device '{name}' already exists."));
                    return Task::none();
                }
                if let Some(device) = self.model.devices.get_mut(index) {
                    let old = device.name.clone();
                    if old == name {
                        return Task::none();
                    }
                    device.name = name.clone();
                    for step in &mut self.model.activity.steps {
                        if step.device == old {
                            step.device = name.clone();
                        }
                    }
                    if self.model.activity.channel_device == old {
                        self.model.activity.channel_device = name.clone();
                    }
                    if let Some(routing) = &mut self.model.routing {
                        if routing.volume.as_deref() == Some(old.as_str()) {
                            routing.volume = Some(name.clone());
                        }
                        if routing.channel.as_deref() == Some(old.as_str()) {
                            routing.channel = Some(name.clone());
                        }
                    }
                    if self.activity_device == old {
                        self.activity_device = name.clone();
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::DeviceKey(index, key) => {
                if self
                    .model
                    .devices
                    .iter()
                    .enumerate()
                    .all(|(other, device)| other == index || device.device_key != key)
                {
                    if let Some(device) = self.model.devices.get_mut(index) {
                        device.device_key = key;
                    }
                    self.persist();
                } else {
                    self.error = Some("That device button is already used.".into());
                }
                Task::none()
            }
            Message::DeleteDevice(index) => {
                if index < self.model.devices.len() {
                    let removed = self.model.devices.remove(index);
                    self.model
                        .activity
                        .steps
                        .retain(|step| step.device != removed.name);
                    if self.model.activity.channel_device == removed.name {
                        self.model.activity.channel_device = self
                            .model
                            .devices
                            .first()
                            .map(|device| device.name.clone())
                            .unwrap_or_default();
                    }
                    if let Some(routing) = &mut self.model.routing {
                        if routing.volume.as_deref() == Some(removed.name.as_str()) {
                            routing.volume = None;
                        }
                        if routing.channel.as_deref() == Some(removed.name.as_str()) {
                            routing.channel = None;
                        }
                    }
                    if let Some(routing) = &self.model.routing
                        && routing.volume.is_none()
                        && routing.channel.is_none()
                    {
                        self.model.routing = None;
                    }
                    if !self
                        .model
                        .devices
                        .iter()
                        .any(|device| device.name == self.activity_device)
                    {
                        self.activity_device = self
                            .model
                            .devices
                            .first()
                            .map(|device| device.name.clone())
                            .unwrap_or_default();
                    }
                    if index < self.selected_device {
                        self.selected_device -= 1;
                    }
                    self.selected_device = self
                        .selected_device
                        .min(self.model.devices.len().saturating_sub(1));
                    self.selected_key = None;
                    self.sync_device_name_draft();
                    self.persist();
                }
                Task::none()
            }
            Message::SelectKey(key) => {
                self.selected_key = Some(key);
                self.save_gui_state();
                Task::none()
            }
            Message::ClearKey => {
                if let Some(key) = self.selected_key.clone() {
                    if let Some(device) = self.model.devices.get_mut(self.selected_device) {
                        device.keys.remove(&key);
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::CommandFilter(filter) => {
                self.command_filter = filter;
                Task::none()
            }
            Message::AssignFirstCommand => self.assign_first_command(),
            Message::AssignCommand(command) => {
                if let Some(key) = self.selected_key.clone() {
                    let routed = self
                        .model
                        .devices
                        .get(self.selected_device)
                        .and_then(|device| routed_target(&self.model, &key, &device.name));
                    if let Some(target) = routed {
                        self.error = Some(format!(
                            "Key '{key}' is routed to '{target}' and cannot be bound here."
                        ));
                        return Task::none();
                    }
                    if let Some(device) = self.model.devices.get_mut(self.selected_device) {
                        device.keys.insert(key, command);
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::NewCommandName(name) => {
                self.new_command.name = name;
                Task::none()
            }
            Message::NewCommandFormat(format) => {
                self.new_command.format = format;
                Task::none()
            }
            Message::NewCommandFile(file) => {
                self.new_command.file = file;
                Task::none()
            }
            Message::NewCommandRemote(remote) => {
                self.new_command.remote = remote;
                Task::none()
            }
            Message::NewCommandButton(button) => {
                self.new_command.button = button;
                Task::none()
            }
            Message::NewCommandHex(hex) => {
                self.new_command.hex = hex;
                Task::none()
            }
            Message::AddCommand => {
                self.add_command();
                Task::none()
            }
            Message::DeleteCommand(name) => {
                if let Some(device) = self.model.devices.get_mut(self.selected_device) {
                    device.commands.remove(&name);
                    device.keys.retain(|_, command| command != &name);
                    for step in &mut self.model.activity.steps {
                        if step.device == device.name {
                            if step.power.as_deref() == Some(name.as_str()) {
                                step.power = None;
                            }
                            if step.input.as_deref() == Some(name.as_str()) {
                                step.input = None;
                            }
                        }
                    }
                }
                self.persist();
                Task::none()
            }
            Message::RoutingVolume(target) => {
                if target.is_some() {
                    self.model.schema_version = SCHEMA_VERSION;
                }
                let routing = self.model.routing.get_or_insert_with(Routing::default);
                routing.volume = target;
                if routing.volume.is_none() && routing.channel.is_none() {
                    self.model.routing = None;
                }
                self.persist();
                Task::none()
            }
            Message::RoutingChannel(target) => {
                if target.is_some() {
                    self.model.schema_version = SCHEMA_VERSION;
                }
                let routing = self.model.routing.get_or_insert_with(Routing::default);
                routing.channel = target;
                if routing.volume.is_none() && routing.channel.is_none() {
                    self.model.routing = None;
                }
                self.persist();
                Task::none()
            }
            Message::SkinChanged(skin) => {
                self.model.skin = skin;
                self.persist();
                Task::none()
            }
            Message::ActivityChannel(name) => {
                self.model.activity.channel_device = name;
                self.persist();
                Task::none()
            }
            Message::ActivityDevice(name) => {
                self.activity_device = name;
                self.activity_power.clear();
                self.activity_input.clear();
                Task::none()
            }
            Message::ActivityPower(value) => {
                self.activity_power = value;
                Task::none()
            }
            Message::ActivityInput(value) => {
                self.activity_input = value;
                Task::none()
            }
            Message::ActivityDelay(value) => {
                self.activity_delay = value;
                Task::none()
            }
            Message::StepDevice(index, name) => {
                if let Some(step) = self.model.activity.steps.get_mut(index) {
                    step.device = name;
                }
                self.persist();
                Task::none()
            }
            Message::StepPower(index, power) => {
                if let Some(step) = self.model.activity.steps.get_mut(index) {
                    step.power = non_empty(power);
                }
                self.persist();
                Task::none()
            }
            Message::StepInput(index, input) => {
                if let Some(step) = self.model.activity.steps.get_mut(index) {
                    step.input = non_empty(input);
                }
                self.persist();
                Task::none()
            }
            Message::StepDelay(index, delay) => {
                if let Some(step) = self.model.activity.steps.get_mut(index) {
                    step.delay_ms = delay.parse().unwrap_or(500);
                }
                self.persist();
                Task::none()
            }
            Message::AddStep => {
                let device = self
                    .model
                    .devices
                    .iter()
                    .find(|device| device.name == self.activity_device)
                    .or_else(|| self.model.devices.first())
                    .map(|device| device.name.clone())
                    .unwrap_or_default();
                if !device.is_empty() {
                    self.model.activity.steps.push(ActivityStep {
                        device,
                        power: non_empty(self.activity_power.clone()),
                        input: non_empty(self.activity_input.clone()),
                        delay_ms: self.activity_delay.parse().unwrap_or(500),
                    });
                    if self.model.activity.channel_device.is_empty() {
                        self.model.activity.channel_device =
                            self.model.activity.steps[0].device.clone();
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::DeleteStep(index) => {
                if index < self.model.activity.steps.len() {
                    self.model.activity.steps.remove(index);
                    self.persist();
                }
                Task::none()
            }
            Message::NewFavouriteChannel(channel) => {
                self.new_favourite_channel = channel;
                Task::none()
            }
            Message::AddFavourite => {
                let capacity = profile::skin_profile(&profile::HARMONY_300, self.model.skin)
                    .map(|skin| skin.favourite_capacity)
                    .unwrap_or(0);
                let key = (1..=capacity).find(|key| {
                    !self
                        .model
                        .favourites
                        .iter()
                        .any(|favourite| usize::from(favourite.key) == *key)
                });
                if self.new_favourite_channel.trim().is_empty() {
                    self.error = Some("Enter a channel number.".into());
                } else if let Some(key) = key {
                    self.model.favourites.push(Favourite {
                        key: key as u8,
                        channel: self.new_favourite_channel.trim().to_string(),
                    });
                    self.new_favourite_channel.clear();
                    self.persist();
                } else {
                    self.error = Some(format!(
                        "Skin {} holds at most {capacity} Favourites.",
                        self.model.skin
                    ));
                }
                Task::none()
            }
            Message::FavouriteChannel(index, channel) => {
                if let Some(favourite) = self.model.favourites.get_mut(index) {
                    favourite.channel = channel;
                }
                self.persist();
                Task::none()
            }
            Message::DeleteFavourite(index) => {
                if index < self.model.favourites.len() {
                    self.model.favourites.remove(index);
                    for (position, favourite) in self.model.favourites.iter_mut().enumerate() {
                        favourite.key = position as u8 + 1;
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::LibraryPath(path) => {
                self.library_dir = path;
                Task::none()
            }
            Message::ScanLibrary => {
                match scan_library(Path::new(&self.library_dir)) {
                    Ok(devices) => {
                        self.library_status = if devices.is_empty() {
                            "No .ir files with named commands found.".to_string()
                        } else {
                            "Scan complete.".to_string()
                        };
                        self.library_selected = (!devices.is_empty()).then_some(0);
                        self.library = devices;
                        self.library_error = None;
                    }
                    Err(error) => {
                        self.library_error = Some(error);
                    }
                }
                Task::none()
            }
            Message::LibrarySearch(search) => {
                self.library_search = search;
                Task::none()
            }
            Message::SelectLibraryDevice(index) => {
                if index < self.library.len() {
                    self.library_selected = Some(index);
                }
                Task::none()
            }
            Message::AddLibraryDevice => {
                let Some(source) = self
                    .library_selected
                    .and_then(|index| self.library.get(index))
                    .cloned()
                else {
                    self.library_error = Some("Choose a library device first.".into());
                    return Task::none();
                };
                if self.model.devices.is_empty() {
                    self.library_error = Some("Choose a device on My Devices first.".into());
                    return Task::none();
                }
                let device_index = self
                    .selected_device
                    .min(self.model.devices.len().saturating_sub(1));
                let target_name = self.model.devices[device_index].name.clone();
                let mut added = 0;
                let mut skipped = 0;
                for name in &source.commands {
                    let target = &mut self.model.devices[device_index];
                    if target.commands.contains_key(name) {
                        skipped += 1;
                        continue;
                    }
                    target.commands.insert(
                        name.clone(),
                        Command {
                            source: IrSource {
                                format: "flipper".into(),
                                file: Some(source.path.clone()),
                                hex: None,
                                remote: None,
                                button: Some(name.clone()),
                                command: None,
                            },
                            sends: None,
                        },
                    );
                    added += 1;
                }
                self.persist();
                self.library_status = format!(
                    "Added {added} command(s) to {target_name}; {skipped} already present."
                );
                self.library_error = None;
                Task::none()
            }
            Message::RestorePath(path) => {
                if self.restore_path != path && self.prepared_kind == Some(PreparedKind::Restore) {
                    self.prepared_kind = None;
                    self.last_prepare = None;
                }
                self.restore_path = path;
                Task::none()
            }
            Message::Save => {
                self.persist();
                if self.saved {
                    self.status = format!("Saved {}", self.model_path.display());
                } else {
                    self.status = "The setup could not be saved.".to_string();
                }
                Task::none()
            }
            Message::Reload => {
                if let Ok(model) = gui_api::load_model(&self.model_path) {
                    self.model = model;
                    self.selected_device = 0;
                    self.selected_key = None;
                    self.activity_device = self
                        .model
                        .devices
                        .first()
                        .map(|device| device.name.clone())
                        .unwrap_or_default();
                    self.container_size = None;
                    self.prepared_kind = None;
                    self.last_prepare = None;
                    self.sync_device_name_draft();
                    self.status = format!("Reloaded {}", self.model_path.display());
                } else {
                    self.error = Some("The saved setup cannot be read.".into());
                }
                Task::none()
            }
            Message::TransferPath(path) => {
                self.model_transfer_path = path;
                Task::none()
            }
            Message::ExportModel => {
                let path = PathBuf::from(self.model_transfer_path.trim());
                if path.as_os_str().is_empty() {
                    self.error = Some("Choose an export path.".into());
                } else {
                    match gui_api::save_model(&path, &self.model) {
                        Ok(()) => self.status = format!("Exported {}", path.display()),
                        Err(error) => self.error = Some(error_text(error)),
                    }
                }
                Task::none()
            }
            Message::ImportModel => {
                let path = PathBuf::from(self.model_transfer_path.trim());
                match gui_api::load_model(&path) {
                    Ok(model) => match gui_api::validate_model(&model) {
                        Ok(()) => {
                            self.model = model;
                            self.selected_device = 0;
                            self.selected_key = None;
                            self.activity_device = self
                                .model
                                .devices
                                .first()
                                .map(|device| device.name.clone())
                                .unwrap_or_default();
                            self.container_size = None;
                            self.sync_device_name_draft();
                            self.persist();
                            self.status = format!("Imported {}", path.display());
                        }
                        Err(error) => self.error = Some(error_text(error)),
                    },
                    Err(error) => self.error = Some(error_text(error)),
                }
                Task::none()
            }
            Message::UseBackup(path) => {
                let selected = path.display().to_string();
                if self.restore_path != selected
                    && self.prepared_kind == Some(PreparedKind::Restore)
                {
                    self.prepared_kind = None;
                    self.last_prepare = None;
                }
                self.restore_path = selected;
                self.page = Page::Remote;
                Task::none()
            }
            Message::RefreshBackups => {
                self.refresh_backups();
                Task::none()
            }
            Message::DeleteBackup(path) => {
                let mut sidecar = path.as_os_str().to_os_string();
                sidecar.push(".json");
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_file(PathBuf::from(sidecar));
                if self.backup_path.as_deref() == Some(path.as_path()) {
                    self.backup_path = None;
                    self.prepared_kind = None;
                    self.last_prepare = None;
                }
                if Path::new(self.restore_path.trim()) == path.as_path() {
                    self.prepared_kind = None;
                    self.last_prepare = None;
                }
                self.refresh_backups();
                Task::none()
            }
            Message::UseDetectedSkin(skin) => {
                if profile::skin_profile(&profile::HARMONY_300, skin).is_some() {
                    self.model.skin = skin;
                    self.error = None;
                    self.persist();
                    self.status = "Setup switched to the connected remote's version.".into();
                } else {
                    self.error = Some(
                        "The connected remote's version is not supported by this setup.".into(),
                    );
                }
                Task::none()
            }
            Message::Reboot(reboot) => {
                self.reboot = reboot;
                self.prepared_kind = None;
                self.last_prepare = None;
                self.save_gui_state();
                Task::none()
            }
            Message::ToggleAdvanced(advanced) => {
                self.advanced = advanced;
                self.save_gui_state();
                Task::none()
            }
            Message::Cancel => {
                self.cancel.store(true, Ordering::Relaxed);
                self.status = "Cancelling after the current step...".into();
                Task::none()
            }
            Message::DismissError => {
                self.error = None;
                Task::none()
            }
        }
    }

    fn shortcut(&mut self, key: Key, modifiers: Modifiers) -> Task<Message> {
        let key = key.as_ref();

        if modifiers.control() && modifiers.alt() {
            match key {
                Key::Named(Named::ArrowLeft) => self.select_relative_device(-1),
                Key::Named(Named::ArrowRight) => self.select_relative_device(1),
                Key::Named(Named::ArrowUp) => self.select_relative_key(-1),
                Key::Named(Named::ArrowDown) => self.select_relative_key(1),
                Key::Named(Named::Backspace) => {
                    self.selected_key = None;
                    self.save_gui_state();
                }
                _ => {}
            }
            return Task::none();
        }

        if modifiers.control() {
            match key {
                Key::Character("1") => self.set_page(Page::Remote),
                Key::Character("2") => self.set_page(Page::Devices),
                Key::Character("3") => self.set_page(Page::KeyMap),
                Key::Character("4") => self.set_page(Page::Activities),
                Key::Character("5") => self.set_page(Page::Favourites),
                Key::Character("6") => self.set_page(Page::Library),
                Key::Character("7") => self.set_page(Page::Settings),
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
                    if self.page == Page::KeyMap {
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
            && self.error.take().is_none()
            && self.selected_key.take().is_some()
        {
            self.save_gui_state();
        }
        Task::none()
    }

    fn set_page(&mut self, page: Page) {
        self.page = page;
        self.save_gui_state();
    }

    fn cycle_page(&mut self, delta: isize) {
        const PAGES: [Page; 7] = [
            Page::Remote,
            Page::Devices,
            Page::KeyMap,
            Page::Activities,
            Page::Favourites,
            Page::Library,
            Page::Settings,
        ];
        let index = PAGES
            .iter()
            .position(|page| *page == self.page)
            .unwrap_or(0);
        let next = (index as isize + delta).rem_euclid(PAGES.len() as isize) as usize;
        self.set_page(PAGES[next]);
    }

    fn select_relative_device(&mut self, delta: isize) {
        if self.model.devices.is_empty() {
            return;
        }
        let len = self.model.devices.len() as isize;
        let next = (self.selected_device as isize + delta).rem_euclid(len) as usize;
        self.selected_device = next;
        self.selected_key = None;
        self.sync_device_name_draft();
        self.save_gui_state();
    }

    fn select_relative_key(&mut self, delta: isize) {
        let keys: Vec<&str> = profile::HARMONY_300
            .key_layout
            .iter()
            .map(|binding| binding.name)
            .collect();
        if keys.is_empty() {
            return;
        }
        let current = self
            .selected_key
            .as_deref()
            .and_then(|selected| keys.iter().position(|key| *key == selected))
            .unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(keys.len() as isize) as usize;
        self.selected_key = Some(keys[next].to_string());
        self.save_gui_state();
    }

    fn assign_first_command(&mut self) -> Task<Message> {
        if self.selected_key.is_none() {
            return Task::none();
        }
        let filter = self.command_filter.to_lowercase();
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

    fn begin(&mut self, status: &str) {
        self.busy = true;
        self.committing = false;
        self.cancel.store(false, Ordering::Relaxed);
        self.error = None;
        self.progress = Some((status.to_string(), 0.0));
        self.status = status.to_string();
        self.phase = match status {
            "Detecting the Remote" => Phase::Detect,
            "Backing up the Remote" => Phase::Backup,
            "Composing" | "Preparing a Write" | "Preparing a Restore" => Phase::Prepare,
            "Writing to the Remote" | "Restoring the Remote" => Phase::Commit,
            _ => self.phase,
        };
    }

    fn finish(&mut self, done: OpDone) {
        match done {
            OpDone::Detected(report) => {
                self.prepared_kind = None;
                self.last_prepare = None;
                if self.model.devices.is_empty()
                    && report.supported
                    && let Some(skin) = report.skin
                    && profile::skin_profile(&profile::HARMONY_300, skin).is_some()
                {
                    self.model.skin = skin;
                    self.persist();
                }
                self.status = if report.supported {
                    if self.advanced {
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
                self.phase = if report.supported {
                    if self.backups.is_empty() {
                        Phase::Backup
                    } else {
                        Phase::Prepare
                    }
                } else {
                    Phase::Detect
                };
                self.remote = Some(report);
            }
            OpDone::BackedUp(report) => {
                self.backup_path = Some(report.container.clone());
                self.prepared_kind = None;
                self.last_prepare = None;
                self.phase = Phase::Prepare;
                self.refresh_backups();
                self.status = if self.advanced {
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
                self.container_size = Some(report.size);
                self.prepared_kind = None;
                self.last_prepare = None;
                self.phase = Phase::Prepare;
                self.status = if self.advanced {
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
                self.backup_path = Some(report.backup.clone());
                self.container_size = Some(report.bytes as u64);
                self.refresh_backups();
                self.last_prepare = Some(report.clone());
                self.prepared_kind = Some(PreparedKind::Write);
                self.phase = Phase::Commit;
                self.page = Page::Remote;
                self.status = if self.advanced {
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
                self.last_prepare = None;
                self.prepared_kind = None;
                self.phase = Phase::ReadBack;
                self.container_size = Some(report.bytes as u64);
                self.status = if self.advanced {
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
                self.last_prepare = Some(report.clone());
                self.prepared_kind = Some(PreparedKind::Restore);
                self.phase = Phase::Commit;
                self.status = if self.advanced {
                    format!(
                        "Ready to restore — {} bytes, {} chunk(s)",
                        report.bytes, report.chunks
                    )
                } else {
                    "Ready to restore the backup.".into()
                };
            }
            OpDone::Restored(report) => {
                self.last_prepare = None;
                self.prepared_kind = None;
                self.phase = Phase::ReadBack;
                self.container_size = Some(report.bytes as u64);
                self.status = if self.advanced {
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

    fn save_gui_state(&self) {
        let state = GuiState {
            page: match self.page {
                Page::Remote => 0,
                Page::Devices => 1,
                Page::KeyMap => 2,
                Page::Activities => 3,
                Page::Favourites => 4,
                Page::Library => 5,
                Page::Settings => 6,
            },
            selected_device: self.selected_device,
            selected_key: self.selected_key.clone(),
            reboot: self.reboot,
            advanced: self.advanced,
            library_dir: Some(self.library_dir.clone()),
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&state) {
            let _ = std::fs::write(&self.gui_path, bytes);
        }
    }

    fn persist(&mut self) {
        self.save_gui_state();
        // A prepared Write or Restore is only trustworthy while `model.json` matches the Model in
        // memory, so a failed save must drop it as well as a successful one.
        self.prepared_kind = None;
        self.last_prepare = None;
        if let Err(error) = gui_api::save_model(&self.model_path, &self.model) {
            self.error = Some(error_text(error));
            self.saved = false;
        } else {
            self.saved = true;
        }
    }

    fn refresh_backups(&mut self) {
        self.backups = list_backups(&self.backups_dir);
    }

    fn backup_path_for(&self, prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.backups_dir.join(format!("{prefix}-{stamp}.bin"))
    }

    fn restore_path(&self) -> Result<PathBuf, String> {
        let value = self.restore_path.trim();
        if value.is_empty() {
            return Err("Choose a Backup path to restore.".into());
        }
        let path = PathBuf::from(value);
        if !path.exists() {
            return Err(format!("Backup {} does not exist.", path.display()));
        }
        Ok(path)
    }

    fn selected_device(&self) -> Option<&Device> {
        self.model.devices.get(self.selected_device)
    }

    fn device_names(&self) -> Vec<String> {
        self.model
            .devices
            .iter()
            .map(|device| device.name.clone())
            .collect()
    }

    /// Keep the in-progress Device name text of the selected row in step with the Model,
    /// so a rejected rename never leaks into another Device or a reloaded Model.
    fn sync_device_name_draft(&mut self) {
        let draft = self
            .model
            .devices
            .get(self.selected_device)
            .map(|device| (self.selected_device, device.name.clone()));
        self.device_name_draft = draft;
    }

    fn validation(&self) -> Option<String> {
        gui_api::validate_model(&self.model).err().map(error_text)
    }

    fn add_device(&mut self) {
        let name = self.new_device_name.trim().to_string();
        if name.is_empty() {
            self.error = Some("Give the device a name.".into());
            return;
        }
        if self.model.devices.iter().any(|device| device.name == name) {
            self.error = Some(format!("Device '{name}' already exists."));
            return;
        }
        if self
            .model
            .devices
            .iter()
            .any(|device| device.device_key == self.new_device_key)
        {
            self.error = Some(format!(
                "Device Key {} is already assigned.",
                self.new_device_key
            ));
            return;
        }
        self.model.devices.push(Device {
            name: name.clone(),
            device_key: self.new_device_key,
            commands: BTreeMap::new(),
            keys: BTreeMap::new(),
        });
        if self.model.activity.channel_device.is_empty() {
            self.model.activity.channel_device = name.clone();
        }
        if self.model.activity.steps.is_empty() {
            self.model.activity.steps.push(ActivityStep {
                device: name.clone(),
                power: Some("power".into()),
                input: None,
                delay_ms: 500,
            });
        }
        self.selected_device = self.model.devices.len() - 1;
        self.activity_device = name.clone();
        self.new_device_name.clear();
        self.sync_device_name_draft();
        self.persist();
        self.status = format!("Added Device '{name}'.");
    }

    fn add_command(&mut self) {
        if self.selected_device().is_none() {
            self.error = Some("Select or add a Device first.".into());
            return;
        }
        let form = self.new_command.clone();
        let name = form.name.trim().to_string();
        if name.is_empty() {
            self.error = Some("Give the command a name.".into());
            return;
        }
        let source = match form_source(&form) {
            Ok(source) => source,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if let Some(device) = self.model.devices.get_mut(self.selected_device) {
            device.commands.insert(
                name.clone(),
                Command {
                    source,
                    sends: None,
                },
            );
            let device_name = device.name.clone();
            for step in &mut self.model.activity.steps {
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
            self.new_command.name.clear();
            self.persist();
            self.status = format!("Added command '{}' to {device_name}.", humanize(&name));
        }
    }

    fn view(&self) -> Element<'_, Message> {
        views::view(self)
    }
}

fn operation_task<F>(work: F) -> Task<Message>
where
    F: FnOnce(&mut dyn FnMut(Progress)) -> Result<OpDone, String> + Send + 'static,
{
    Task::sip(operation(work), Message::Progress, Message::Done)
}

enum OperationEvent {
    Progress(Progress),
    Done(Result<OpDone, String>),
}

fn operation<F>(work: F) -> impl Straw<OpDone, Progress, String>
where
    F: FnOnce(&mut dyn FnMut(Progress)) -> Result<OpDone, String> + Send + 'static,
{
    sipper(async move |mut progress| {
        let (sender, mut receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            let mut sink = |event: Progress| {
                let _ = sender.unbounded_send(OperationEvent::Progress(event));
            };
            let result = work(&mut sink);
            let _ = sender.unbounded_send(OperationEvent::Done(result));
        });
        while let Some(event) = receiver.next().await {
            match event {
                OperationEvent::Progress(event) => {
                    let _ = progress.send(event).await;
                }
                OperationEvent::Done(result) => return result,
            }
        }
        Err("operation ended without a result".into())
    })
}

fn check_cancel(cancel: &Arc<AtomicBool>) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Cancelled before the commit.".into())
    } else {
        Ok(())
    }
}

fn app_data_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("remote-rebirth");
    }
    if let Some(home) = std::env::var_os("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".local/share/remote-rebirth");
    }
    PathBuf::from(".remote-rebirth")
}

fn form_source(form: &CommandForm) -> Result<IrSource, String> {
    let file = form.file.trim();
    match form.format.as_str() {
        "pronto" => {
            let hex = form.hex.trim();
            if !hex.is_empty() {
                Ok(IrSource {
                    format: "pronto".into(),
                    file: None,
                    hex: Some(hex.into()),
                    remote: None,
                    button: None,
                    command: None,
                })
            } else if !file.is_empty() {
                Ok(IrSource {
                    format: "pronto".into(),
                    file: Some(PathBuf::from(file)),
                    hex: None,
                    remote: None,
                    button: None,
                    command: None,
                })
            } else {
                Err("Pronto needs inline hex or a file.".into())
            }
        }
        "lirc" => {
            if file.is_empty() || form.remote.trim().is_empty() || form.button.trim().is_empty() {
                return Err("LIRC needs a file, remote and button.".into());
            }
            Ok(IrSource {
                format: "lirc".into(),
                file: Some(PathBuf::from(file)),
                hex: None,
                remote: Some(form.remote.trim().into()),
                button: Some(form.button.trim().into()),
                command: None,
            })
        }
        "flipper" => {
            if file.is_empty() || form.button.trim().is_empty() {
                return Err("Flipper needs a file and a command.".into());
            }
            Ok(IrSource {
                format: "flipper".into(),
                file: Some(PathBuf::from(file)),
                hex: None,
                remote: None,
                button: None,
                command: Some(form.button.trim().into()),
            })
        }
        other => Err(format!("Unknown IR format '{other}'.")),
    }
}

fn device_key_labels() -> Vec<String> {
    profile::HARMONY_300
        .device_key_labels
        .iter()
        .map(|label| label.to_string())
        .collect()
}

fn device_key_label(key: u8) -> String {
    profile::HARMONY_300
        .device_key_labels
        .get(usize::from(key).saturating_sub(1))
        .copied()
        .unwrap_or("?")
        .to_string()
}

fn device_key_from_label(label: &str) -> u8 {
    profile::HARMONY_300
        .device_key_labels
        .iter()
        .position(|candidate| *candidate == label)
        .map(|index| index as u8 + 1)
        .unwrap_or(1)
}

fn command_formats() -> Vec<String> {
    vec![
        "Pronto code".into(),
        "LIRC file".into(),
        "Flipper file".into(),
    ]
}

fn command_format_label(format: &str) -> String {
    match format {
        "lirc" => "LIRC file",
        "flipper" => "Flipper file",
        _ => "Pronto code",
    }
    .to_string()
}

fn command_format_from_label(label: &str) -> String {
    match label {
        "LIRC file" => "lirc",
        "Flipper file" => "flipper",
        _ => "pronto",
    }
    .to_string()
}

fn routed_target(model: &Model, key: &str, device: &str) -> Option<String> {
    let routing = model.routing.as_ref()?;
    if Routing::VOLUME_KEYS.contains(&key)
        && let Some(target) = &routing.volume
        && target != device
    {
        return Some(target.clone());
    }
    if Routing::CHANNEL_KEYS.contains(&key)
        && let Some(target) = &routing.channel
        && target != device
    {
        return Some(target.clone());
    }
    None
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn none_if_placeholder(value: String) -> Option<String> {
    if value == "Each device itself" {
        None
    } else {
        Some(value)
    }
}

fn humanize(value: &str) -> String {
    let mut text = String::new();
    for word in value.split('_') {
        if !text.is_empty() {
            text.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            text.extend(first.to_uppercase());
            text.extend(chars);
        }
    }
    text
}

fn short_digest(digest: &str) -> String {
    digest.chars().take(12).collect()
}

/// Renders an optional protocol number for technical details without `Some(...)`.
fn plain_number(value: Option<u8>) -> String {
    value.map_or_else(|| "?".into(), |value| value.to_string())
}

fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn step_label(step: &str) -> &str {
    match step {
        "load" => "Loading",
        "detect" => "Detecting",
        "verify" => "Verifying Backup and current Configuration",
        "compose" => "Composing",
        "backup" => "Backing up",
        "prepare" => "Preparing",
        "commit" => "Committing",
        "reset" => "Resetting the file system",
        "open" => "Opening /cfg/usercfg",
        "check" => "Checking the checksum",
        "flush" => "Flushing",
        "close" => "Closing",
        "read-back" => "Reading back",
        "reboot" => "Rebooting",
        "done" => "Done",
        other => other,
    }
}

fn list_backups(directory: &Path) -> Vec<PathBuf> {
    let mut backups: Vec<PathBuf> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("bin"))
        .collect();
    backups.sort();
    backups.reverse();
    backups
}

fn scan_library(root: &Path) -> Result<Vec<LibraryDevice>, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a folder.", root.display()));
    }
    let mut directories = vec![root.to_path_buf()];
    let mut devices = Vec::new();
    while let Some(directory) = directories.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("ir") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut commands: Vec<String> = text
                .lines()
                .filter_map(|line| line.strip_prefix("name:"))
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect();
            commands.sort();
            commands.dedup();
            if commands.is_empty() {
                continue;
            }
            let relative = path.strip_prefix(root).ok();
            let brand = relative
                .and_then(|relative| relative.parent())
                .and_then(|parent| parent.components().next())
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .unwrap_or_else(|| "Local".into());
            let model = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            devices.push(LibraryDevice {
                brand,
                model,
                path,
                commands,
            });
            if devices.len() >= 20_000 {
                break;
            }
        }
    }
    devices.sort_by(|left, right| {
        left.brand
            .cmp(&right.brand)
            .then(left.model.cmp(&right.model))
    });
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_scan_indexes_brand_model_and_commands() {
        let directory = tempfile::tempdir().expect("tempdir");
        let brand = directory.path().join("Samsung");
        std::fs::create_dir_all(&brand).expect("brand dir");
        std::fs::write(
            brand.join("TV.ir"),
            "Filetype: IR signals file\nVersion: 1\n#\nname: Power\ntype: parsed\n#\nname: Volume Up\ntype: raw\n",
        )
        .expect("write library file");
        let devices = scan_library(directory.path()).expect("scan");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].brand, "Samsung");
        assert_eq!(devices[0].model, "TV");
        assert_eq!(devices[0].commands, ["Power", "Volume Up"]);
    }

    #[test]
    fn library_import_adds_all_commands_to_selected_device() {
        let directory = tempfile::tempdir().expect("tempdir");
        let brand = directory.path().join("Samsung");
        std::fs::create_dir_all(&brand).expect("brand dir");
        std::fs::write(
            brand.join("TV.ir"),
            "Filetype: IR signals file\nVersion: 1\nname: Power\ntype: parsed\nname: Volume Up\ntype: raw\n",
        )
        .expect("write library file");
        let (mut app, _) = App::at(directory.path().join("data"));
        app.library = scan_library(directory.path()).expect("scan");
        app.library_selected = Some(0);
        app.model.devices.push(Device {
            name: "Living Room TV".into(),
            device_key: 1,
            commands: BTreeMap::new(),
            keys: BTreeMap::new(),
        });
        app.selected_device = 0;
        let _ = app.update(Message::AddLibraryDevice);
        let device = &app.model.devices[0];
        assert!(device.commands.contains_key("Power"));
        assert!(device.commands.contains_key("Volume Up"));
        assert_eq!(device.commands["Power"].source.format, "flipper");
        assert_eq!(
            device.commands["Power"].source.button.as_deref(),
            Some("Power")
        );
    }

    #[test]
    fn blank_app_builds_a_valid_one_device_project() {
        let directory = tempfile::tempdir().expect("tempdir");
        let (mut app, _) = App::at(directory.path().to_path_buf());
        app.new_device_name = "TV".into();
        app.new_device_key = 1;
        app.add_device();
        app.new_command.name = "power".into();
        app.new_command.format = "pronto".into();
        app.new_command.hex = "0000 006D 0002 0000 0020 0020 0020 0020".into();
        app.add_command();
        app.selected_key = Some("power".into());
        let _ = app.update(Message::AssignCommand("power".into()));
        gui_api::validate_model(&app.model).expect("a valid one-Device project");
        assert!(directory.path().join("model.json").exists());
        assert!(directory.path().join("gui.json").exists());
    }

    #[test]
    fn unreadable_model_is_kept_before_the_blank_project_can_be_saved() {
        let directory = tempfile::tempdir().expect("tempdir");
        let model_path = directory.path().join("model.json");
        std::fs::write(&model_path, b"{ not a Model }").expect("write an unreadable Model");
        let (app, _) = App::at(directory.path().to_path_buf());
        assert!(!model_path.exists(), "the unreadable file stays untouched");
        let kept: Vec<_> = std::fs::read_dir(directory.path())
            .expect("read_dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("model.json.unreadable-"))
            .collect();
        assert_eq!(kept.len(), 1, "the unreadable file was moved aside");
        assert!(app.error.is_some(), "the loss is reported");
    }

    #[test]
    fn source_form_requires_the_format_fields() {
        let mut form = CommandForm {
            format: "lirc".into(),
            file: "tv.conf".into(),
            ..CommandForm::default()
        };
        assert!(form_source(&form).is_err());
        form.remote = "TV".into();
        form.button = "Power".into();
        assert!(form_source(&form).is_ok());

        let mut flipper = CommandForm {
            format: "flipper".into(),
            file: "tv.ir".into(),
            ..CommandForm::default()
        };
        assert!(form_source(&flipper).is_err());
        flipper.button = "Power".into();
        assert!(form_source(&flipper).is_ok());
    }

    #[test]
    fn sidebar_and_detect_buttons_emit_messages() {
        let directory = tempfile::tempdir().expect("tempdir");
        let (app, _) = App::at(directory.path().to_path_buf());
        let mut ui = iced_test::simulator(app.view());
        ui.click("My Devices").expect("click My Devices");
        let messages: Vec<_> = ui.into_messages().collect();
        assert!(matches!(
            messages.first(),
            Some(Message::Page(Page::Devices))
        ));

        let (app, _) = App::at(directory.path().to_path_buf());
        let mut ui = iced_test::simulator(app.view());
        ui.click("Find remote").expect("click Find remote");
        let messages: Vec<_> = ui.into_messages().collect();
        assert!(matches!(messages.first(), Some(Message::Detect)));
    }

    #[test]
    fn lifecycle_phase_follows_the_operation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let (mut app, _) = App::at(directory.path().to_path_buf());
        assert_eq!(app.phase, Phase::Detect);
        app.begin("Backing up the Remote");
        assert_eq!(app.phase, Phase::Backup);
        app.begin("Preparing a Write");
        assert_eq!(app.phase, Phase::Prepare);
        app.begin("Writing to the Remote");
        assert_eq!(app.phase, Phase::Commit);
        app.finish(OpDone::Committed(WriteResult {
            operation: "write",
            commit: true,
            container: None,
            backup: PathBuf::from("backup.bin"),
            size: 0,
            backup_sha256: "0".repeat(64),
            bytes: 0,
            chunks: 0,
            data_packets: 0,
            reboot: false,
            journal: None,
        }));
        assert_eq!(app.phase, Phase::ReadBack);
        app.finish(OpDone::Detected(DetectReport {
            supported: false,
            model: "Harmony 200".into(),
            family: None,
            profile: None,
            architecture: None,
            skin: None,
            firmware_version: None,
            hardware_version: None,
            guid: None,
            serial: None,
            usb: remote_rebirth_core::gui_api::UsbIdentity {
                vendor_id: 0,
                product_id: 0,
                bcd_device: 0,
                product: None,
                serial: None,
                bus: 0,
                address: 0,
            },
        }));
        assert_eq!(app.phase, Phase::Detect);
    }

    #[test]
    fn keyboard_shortcuts_change_chapters_and_dismiss() {
        let directory = tempfile::tempdir().expect("tempdir");
        let (mut app, _) = App::at(directory.path().to_path_buf());
        assert_eq!(app.page, Page::Remote);
        let _ = app.update(Message::Shortcut(
            Key::Character("3".into()),
            Modifiers::CTRL,
        ));
        assert_eq!(app.page, Page::KeyMap);
        let _ = app.update(Message::Shortcut(
            Key::Named(Named::PageDown),
            Modifiers::CTRL,
        ));
        assert_eq!(app.page, Page::Activities);
        let _ = app.update(Message::Shortcut(
            Key::Named(Named::PageUp),
            Modifiers::CTRL,
        ));
        assert_eq!(app.page, Page::KeyMap);
        app.error = Some("connection lost".into());
        let _ = app.update(Message::Shortcut(
            Key::Named(Named::Escape),
            Modifiers::NONE,
        ));
        assert!(app.error.is_none());
    }

    #[test]
    fn routing_is_reported_only_on_non_target_devices() {
        let mut model = gui_api::blank_model(79);
        model.routing = Some(Routing {
            volume: Some("TV".into()),
            channel: None,
        });
        assert_eq!(
            routed_target(&model, "volume_up", "Cable"),
            Some("TV".into())
        );
        assert_eq!(routed_target(&model, "volume_up", "TV"), None);
        assert_eq!(routed_target(&model, "channel_up", "Cable"), None);
    }
}
