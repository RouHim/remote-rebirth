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
use iced::task::{Straw, sipper};
use iced::widget::{
    Column, Row, button, checkbox, column, container, pick_list, progress_bar, row, scrollable,
    text, text_input,
};
use iced::{
    Alignment, Background, Border, Color, Element, Fill, Font, Length, Shadow, Size, Task, Theme,
    Vector,
};
use remote_rebirth_core::gui_api::{self, BackupResult, DetectReport, WriteResult};
use remote_rebirth_core::model::{
    ActivityStep, Command, Device, Favourite, IrSource, Model, Routing, SCHEMA_VERSION,
};
use remote_rebirth_core::{HidBackend, Progress, SystemClock, profile};
use serde::{Deserialize, Serialize};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Remote Rebirth")
        .theme(App::theme)
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
enum Page {
    Remote,
    Devices,
    KeyMap,
    Activities,
    Favourites,
    Settings,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::Remote,
        Page::Devices,
        Page::KeyMap,
        Page::Activities,
        Page::Favourites,
        Page::Settings,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::Remote => "Remote",
            Page::Devices => "My Devices",
            Page::KeyMap => "Buttons",
            Page::Activities => "Watch TV",
            Page::Favourites => "Favourites",
            Page::Settings => "Settings",
        }
    }
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
    SelectDeviceNamed(String),
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
}

fn default_page() -> u8 {
    0
}

fn default_reboot() -> bool {
    true
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            page: 0,
            selected_device: 0,
            selected_key: None,
            reboot: true,
            advanced: false,
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
            5 => Page::Settings,
            _ => Page::Remote,
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
                activity_power: "power".into(),
                activity_input: "input".into(),
                activity_delay: "500".into(),
                new_favourite_channel: String::new(),
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
            },
            Task::none(),
        )
    }

    fn theme(&self) -> Theme {
        Theme::custom(
            "Remote Rebirth",
            iced::theme::Palette {
                background: Color::from_rgb(0.965, 0.969, 0.976),
                text: Color::from_rgb(0.105, 0.125, 0.155),
                primary: Color::from_rgb(0.145, 0.365, 0.705),
                success: Color::from_rgb(0.135, 0.545, 0.345),
                warning: Color::from_rgb(0.760, 0.500, 0.105),
                danger: Color::from_rgb(0.760, 0.205, 0.205),
            },
        )
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
            Message::SelectDeviceNamed(name) => {
                if let Some(index) = self
                    .model
                    .devices
                    .iter()
                    .position(|device| device.name == name)
                {
                    self.selected_device = index;
                    self.selected_key = None;
                    self.sync_device_name_draft();
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

    fn begin(&mut self, status: &str) {
        self.busy = true;
        self.committing = false;
        self.cancel.store(false, Ordering::Relaxed);
        self.error = None;
        self.progress = Some((status.to_string(), 0.0));
        self.status = status.to_string();
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
                self.remote = Some(report);
            }
            OpDone::BackedUp(report) => {
                self.backup_path = Some(report.container.clone());
                self.prepared_kind = None;
                self.last_prepare = None;
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
                Page::Settings => 5,
            },
            selected_device: self.selected_device,
            selected_key: self.selected_key.clone(),
            reboot: self.reboot,
            advanced: self.advanced,
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

    fn selected_device_name(&self) -> Option<String> {
        self.selected_device().map(|device| device.name.clone())
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
        let sidebar = self.view_sidebar();
        let content = match self.page {
            Page::Remote => self.view_remote(),
            Page::Devices => self.view_devices(),
            Page::KeyMap => self.view_keymap(),
            Page::Activities => self.view_activities(),
            Page::Favourites => self.view_favourites(),
            Page::Settings => self.view_settings(),
        };
        let body = row![
            sidebar,
            scrollable(container(content).padding(24).width(Fill)).width(Fill)
        ]
        .height(Fill);
        let footer = self.view_footer();
        column![body, footer].height(Fill).into()
    }

    fn view_sidebar(&self) -> Element<'_, Message> {
        let mut items: Vec<Element<'_, Message>> = vec![
            text("Remote Rebirth").size(22).font(Font::MONOSPACE).into(),
            text("Harmony 300").size(12).into(),
        ];
        for page in Page::ALL {
            let selected = page == self.page;
            let mut item = button(text(page.title()).size(15))
                .width(Fill)
                .padding([10, 12]);
            if !self.busy {
                item = item.on_press(Message::Page(page));
            }
            if selected {
                item = item.style(button::primary);
            } else {
                item = item.style(button::secondary);
            }
            items.push(item.into());
        }
        items.push(iced::widget::Space::new().height(Length::Fill).into());
        container(Column::with_children(items).spacing(8))
            .width(Length::Fixed(220.0))
            .height(Fill)
            .padding(16)
            .style(sidebar_style)
            .into()
    }

    fn view_footer(&self) -> Element<'_, Message> {
        let identity = match &self.remote {
            Some(remote) if self.advanced => format!(
                "{} · Skin {} · firmware {} · {}",
                remote.model,
                plain_number(remote.skin),
                remote.firmware_version.as_deref().unwrap_or("?"),
                remote.usb.product.as_deref().unwrap_or("USB")
            ),
            Some(remote) => format!("{} connected", remote.model),
            None => "No remote connected".to_string(),
        };
        let validation = match (self.advanced, self.validation()) {
            (true, Some(error)) => error,
            (false, Some(_)) => "Setup incomplete".to_string(),
            (_, None) => "Ready".to_string(),
        };
        let saved = if self.saved {
            "All changes saved"
        } else {
            "Unsaved changes"
        };
        let container_state = if self.advanced {
            self.container_size
                .map(|size| format!(" · {} B", size))
                .unwrap_or_else(|| " · not checked".into())
        } else {
            String::new()
        };
        let mut status = row![
            text(identity).size(12),
            iced::widget::Space::new().width(Length::Fill),
            text(format!("{saved}{container_state}")).size(12),
            text(validation).size(12),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        if let Some((label, fraction)) = &self.progress {
            status = status.push(
                container(progress_bar(0.0..=1.0, *fraction).girth(6.0))
                    .width(Length::Fixed(160.0)),
            );
            status = status.push(text(label).size(12));
        }
        if self.busy && self.committing {
            status = status.push(text("Commit in progress — Cancel disabled").size(12));
        } else if self.busy {
            status = status.push(
                button("Cancel")
                    .on_press(Message::Cancel)
                    .style(button::danger),
            );
        }
        container(status)
            .padding([8, 16])
            .width(Fill)
            .style(footer_style)
            .into()
    }

    fn view_remote(&self) -> Element<'_, Message> {
        let mut content: Vec<Element<'_, Message>> = vec![
            text("Remote").size(28).into(),
            text(&self.status).size(13).into(),
        ];
        if let Some(error) = &self.error {
            content.push(error_banner(error, self.advanced));
            let mut recovery: Vec<Element<'_, Message>> = vec![
                text("If something goes wrong, restore your last backup.")
                    .size(13)
                    .into(),
            ];
            if self.advanced {
                let journal = remote_rebirth_core::state_dir()
                    .join("journal.jsonl")
                    .display()
                    .to_string();
                let backup = self
                    .backup_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "No backup saved in this session.".into());
                recovery.push(data_line("Activity log", &journal));
                recovery.push(data_line("Backup", &backup));
            }
            if let Some(path) = &self.backup_path {
                recovery.push(
                    button("Restore this backup")
                        .on_press(Message::UseBackup(path.clone()))
                        .style(button::secondary)
                        .into(),
                );
            }
            content.push(card(
                "Recovery",
                Column::with_children(recovery).spacing(6).into(),
            ));
        }
        let identity = match &self.remote {
            Some(remote) if self.advanced => Column::with_children(vec![
                data_line("Model", &remote.model),
                data_line("Family", remote.family.unwrap_or("unknown")),
                data_line("Architecture", &plain_number(remote.architecture)),
                data_line("Skin", &plain_number(remote.skin)),
                data_line(
                    "Firmware",
                    remote.firmware_version.as_deref().unwrap_or("unknown"),
                ),
                data_line(
                    "Hardware",
                    remote.hardware_version.as_deref().unwrap_or("unknown"),
                ),
                data_line("GUID", remote.guid.as_deref().unwrap_or("unknown")),
                data_line("Serial", remote.serial.as_deref().unwrap_or("unknown")),
                data_line(
                    "USB",
                    &format!(
                        "{} · {:04X}:{:04X} · bus {} address {}",
                        remote.usb.product.as_deref().unwrap_or("Logitech"),
                        remote.usb.vendor_id,
                        remote.usb.product_id,
                        remote.usb.bus,
                        remote.usb.address
                    ),
                ),
            ])
            .spacing(6)
            .into(),
            Some(remote) => Column::with_children(vec![
                text(&remote.model).size(20).into(),
                data_line(
                    "Firmware",
                    remote.firmware_version.as_deref().unwrap_or("unknown"),
                ),
                text("Connected and ready.").size(13).into(),
            ])
            .spacing(6)
            .into(),
            None => text("Connect the Harmony 300 with the USB cable, then choose Find remote.")
                .size(14)
                .into(),
        };
        content.push(card(
            if self.advanced {
                "Remote details"
            } else {
                "Remote"
            },
            identity,
        ));
        if let Some(remote) = &self.remote
            && let Some(skin) = remote.skin
            && skin != self.model.skin
        {
            let version_message: Element<'_, Message> = if self.advanced {
                text(format!(
                    "The connected remote reports Skin {skin}; the setup uses Skin {}.",
                    self.model.skin
                ))
                .size(13)
                .into()
            } else {
                text("This setup was made for a different remote version.")
                    .size(13)
                    .into()
            };
            content.push(card(
                "Remote version",
                column![
                    version_message,
                    button("Use the connected remote's version")
                        .on_press(Message::UseDetectedSkin(skin))
                        .style(button::primary),
                ]
                .spacing(8)
                .into(),
            ));
        }
        let mut backup_rows: Vec<Element<'_, Message>> = Vec::new();
        for (index, path) in self.backups.iter().take(6).enumerate() {
            backup_rows.push(
                row![
                    text(format!("Backup {}", index + 1)).size(12),
                    text(backup_detail(path, self.advanced)).size(11),
                    iced::widget::Space::new().width(Length::Fill),
                    button("Use for restore")
                        .on_press(Message::UseBackup(path.clone()))
                        .style(button::secondary),
                    button("Remove")
                        .on_press(Message::DeleteBackup(path.clone()))
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        let mut backup_content: Vec<Element<'_, Message>> = vec![
            text("Save the remote's current setup before changing anything.")
                .size(13)
                .into(),
            row![
                action("Find remote", Message::Detect, self.busy),
                action("Save backup", Message::Backup, self.busy),
                action("Refresh", Message::RefreshBackups, self.busy),
            ]
            .spacing(8)
            .into(),
        ];
        if self.advanced
            && let Some(path) = &self.backup_path
        {
            backup_content.push(data_line("Current backup", &path.display().to_string()));
        }
        if backup_rows.is_empty() {
            let empty: Element<'_, Message> = text("No backups saved yet.").size(12).into();
            backup_content.push(empty);
        } else {
            backup_content.push(Column::with_children(backup_rows).spacing(4).into());
        }
        content.push(card(
            "Backup",
            Column::with_children(backup_content).spacing(10).into(),
        ));
        let prepared = self
            .last_prepare
            .as_ref()
            .filter(|_| self.prepared_kind == Some(PreparedKind::Write))
            .map(|report| {
                if self.advanced {
                    format!(
                        "Ready to update — {} bytes, {} chunk(s), {} data packet(s), backup {}, reboot {}",
                        report.bytes,
                        report.chunks,
                        report.data_packets,
                        short_digest(&report.backup_sha256),
                        if report.reboot { "yes" } else { "no" }
                    )
                } else {
                    "Ready to update. A backup was saved first.".into()
                }
            });
        let invalid = self.validation().is_some();
        content.push(card(
            "Update remote",
            column![
                text(prepared.unwrap_or_else(|| {
                    "Check the new setup, then write it to the remote. A backup is saved first."
                        .into()
                }))
                .size(13),
                text(match &self.remote {
                    Some(remote) => format!("Remote: {}", remote.model),
                    None => "Connect the remote before updating it.".into(),
                })
                .size(12),
                checkbox(self.reboot)
                    .label("Restart remote after update")
                    .on_toggle(Message::Reboot),
                row![
                    action("Check setup", Message::Compose, self.busy || invalid),
                    action("Review update", Message::PrepareWrite, self.busy || invalid),
                    action(
                        "Update remote",
                        Message::CommitWrite,
                        self.busy || invalid || self.prepared_kind != Some(PreparedKind::Write)
                    ),
                ]
                .spacing(8),
            ]
            .spacing(10)
            .into(),
        ));
        let mut restore_content: Vec<Element<'_, Message>> = vec![
            text("Put a saved backup back on the remote.")
                .size(13)
                .into(),
        ];
        if self.advanced {
            restore_content.push(
                text_input("Path to backup file", &self.restore_path)
                    .on_input(Message::RestorePath)
                    .width(Fill)
                    .into(),
            );
        } else if self.restore_path.trim().is_empty() {
            restore_content.push(
                text("Choose a backup from the list above first.")
                    .size(12)
                    .into(),
            );
        }
        restore_content.push(
            row![
                action("Review restore", Message::PrepareRestore, self.busy),
                action(
                    "Restore remote",
                    Message::CommitRestore,
                    self.busy || self.prepared_kind != Some(PreparedKind::Restore)
                ),
            ]
            .spacing(8)
            .into(),
        );
        content.push(card(
            "Restore a backup",
            Column::with_children(restore_content).spacing(10).into(),
        ));
        Column::with_children(content).spacing(16).into()
    }

    fn view_devices(&self) -> Element<'_, Message> {
        let mut content: Vec<Element<'_, Message>> = vec![text("My Devices").size(28).into()];
        if let Some(error) = &self.error {
            content.push(error_banner(error, self.advanced));
        }
        let mut devices: Vec<Element<'_, Message>> = Vec::new();
        for (index, device) in self.model.devices.iter().enumerate() {
            let selected = index == self.selected_device;
            let name_text = match &self.device_name_draft {
                Some((draft_index, draft)) if *draft_index == index => draft.clone(),
                _ => device.name.clone(),
            };
            let mut select = button(text(if selected { "Selected" } else { "Select" }).size(14))
                .padding([8, 10]);
            if !self.busy {
                select = select.on_press(Message::SelectDevice(index));
            }
            if selected {
                select = select.style(button::primary);
            } else {
                select = select.style(button::secondary);
            }
            devices.push(
                row![
                    text_input("Device name", &name_text)
                        .on_input(move |name| Message::DeviceName(index, name))
                        .width(Length::Fixed(190.0)),
                    pick_list(
                        device_key_labels(),
                        Some(device_key_label(device.device_key)),
                        move |label| Message::DeviceKey(index, device_key_from_label(&label))
                    ),
                    text(format!(
                        "{} command{}",
                        device.commands.len(),
                        if device.commands.len() == 1 { "" } else { "s" }
                    ))
                    .size(12),
                    select,
                    button("Remove")
                        .on_press_maybe((!self.busy).then_some(Message::DeleteDevice(index)))
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        content.push(card(
            "My devices",
            if devices.is_empty() {
                text("No Devices yet.").size(14).into()
            } else {
                Column::with_children(devices).spacing(6).into()
            },
        ));
        content.push(card(
            "Add a device",
            column![
                text_input("Name, for example TV", &self.new_device_name)
                    .on_input(Message::NewDeviceName)
                    .width(Fill),
                row![
                    pick_list(
                        device_key_labels(),
                        Some(device_key_label(self.new_device_key)),
                        |label| Message::NewDeviceKey(device_key_from_label(&label))
                    ),
                    action("Add device", Message::AddDevice, self.busy),
                ]
                .spacing(8),
            ]
            .spacing(10)
            .into(),
        ));
        if let Some(device) = self.selected_device() {
            let referenced = remote_rebirth_core::model::referenced_commands(&self.model);
            let mut commands: Vec<Element<'_, Message>> = Vec::new();
            for (name, command) in &device.commands {
                let sent = referenced.contains(&(device.name.clone(), name.clone()));
                let source = format!(
                    "{} {}",
                    command.source.format,
                    command
                        .source
                        .file
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .or_else(|| command.source.hex.as_ref().map(|_| "inline hex".into()))
                        .unwrap_or_default()
                );
                let mut row = row![text(humanize(name)).size(14)]
                    .spacing(8)
                    .align_y(Alignment::Center);
                if !sent {
                    row = row.push(text("Unused").size(11));
                }
                row = row.push(iced::widget::Space::new().width(Length::Fill));
                if self.advanced {
                    row = row.push(text(source).size(11).font(Font::MONOSPACE));
                }
                row = row.push(
                    button("Remove")
                        .on_press_maybe(
                            (!self.busy).then_some(Message::DeleteCommand(name.clone())),
                        )
                        .style(button::secondary),
                );
                commands.push(row.into());
            }
            content.push(card(
                format!("Commands for {}", device.name),
                if commands.is_empty() {
                    text("No commands yet. Add at least one.").size(14).into()
                } else {
                    Column::with_children(commands).spacing(6).into()
                },
            ));
        }
        content.push(card(
            "Add a command",
            column![
                text_input("Name, for example Power", &self.new_command.name)
                    .on_input(Message::NewCommandName)
                    .width(Fill),
                row![
                    pick_list(
                        command_formats(),
                        Some(command_format_label(&self.new_command.format)),
                        |label| Message::NewCommandFormat(command_format_from_label(&label))
                    ),
                    text_input("Code file", &self.new_command.file)
                        .on_input(Message::NewCommandFile)
                        .width(Fill),
                ]
                .spacing(8),
                if self.new_command.format == "pronto" {
                    let input: Element<'_, Message> =
                        text_input("Paste Pronto code here", &self.new_command.hex)
                            .on_input(Message::NewCommandHex)
                            .width(Fill)
                            .into();
                    input
                } else if self.new_command.format == "lirc" {
                    row![
                        text_input("Remote name", &self.new_command.remote)
                            .on_input(Message::NewCommandRemote)
                            .width(Fill),
                        text_input("Button name", &self.new_command.button)
                            .on_input(Message::NewCommandButton)
                            .width(Fill),
                    ]
                    .spacing(8)
                    .into()
                } else {
                    text_input("Command name", &self.new_command.button)
                        .on_input(Message::NewCommandButton)
                        .width(Fill)
                        .into()
                },
                action("Add command", Message::AddCommand, self.busy),
            ]
            .spacing(10)
            .into(),
        ));
        Column::with_children(content).spacing(16).into()
    }

    fn view_keymap(&self) -> Element<'_, Message> {
        let mut content: Vec<Element<'_, Message>> = vec![text("Buttons").size(28).into()];
        if let Some(error) = &self.error {
            content.push(error_banner(error, self.advanced));
        }
        let names = self.device_names();
        content.push(
            row![
                text("For").size(13),
                pick_list(
                    names.clone(),
                    self.selected_device_name(),
                    Message::SelectDeviceNamed
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
        let mut routing_options = vec!["Each device itself".to_string()];
        routing_options.extend(names.clone());
        content.push(card(
            "Volume and channel buttons",
            column![
                text(
                    "Choose which device these buttons should always control, whichever device you are using."
                )
                .size(12),
                row![
                    text("Volume buttons").size(13).width(Length::Fixed(140.0)),
                    pick_list(
                        routing_options.clone(),
                        Some(
                            self.model
                                .routing
                                .as_ref()
                                .and_then(|routing| routing.volume.clone())
                                .unwrap_or_else(|| "Each device itself".into())
                        ),
                        |value| Message::RoutingVolume(none_if_placeholder(value)),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text("Channel buttons").size(13).width(Length::Fixed(140.0)),
                    pick_list(
                        routing_options,
                        Some(
                            self.model
                                .routing
                                .as_ref()
                                .and_then(|routing| routing.channel.clone())
                                .unwrap_or_else(|| "Each device itself".into())
                        ),
                        |value| Message::RoutingChannel(none_if_placeholder(value)),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(10)
            .into(),
        ));
        let Some(device) = self.selected_device() else {
            content.push(text("Add a device first.").size(14).into());
            return Column::with_children(content).spacing(16).into();
        };
        content.push(text(format!("Buttons for {}", device.name)).size(16).into());
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        for chunk in profile::HARMONY_300.key_layout.chunks(5) {
            let mut buttons: Vec<Element<'_, Message>> = Vec::new();
            for binding in chunk {
                let bound = device.keys.contains_key(binding.name);
                let inherited = routed_target(&self.model, binding.name, &device.name);
                let label = if let Some(ref target) = inherited {
                    format!("{}\n({target})", humanize(binding.name))
                } else {
                    format!(
                        "{}{}",
                        humanize(binding.name),
                        if bound { " •" } else { "" }
                    )
                };
                let mut key_button = button(text(label).size(11))
                    .width(Length::Fixed(128.0))
                    .height(Length::Fixed(40.0))
                    .padding(4)
                    .style(button::secondary);
                if !self.busy && inherited.is_none() {
                    key_button = key_button.on_press(Message::SelectKey(binding.name.into()));
                }
                if self.selected_key.as_deref() == Some(binding.name) {
                    key_button = key_button.style(button::primary);
                }
                buttons.push(key_button.into());
            }
            rows.push(Row::with_children(buttons).spacing(6).into());
        }
        content.push(card(
            "Remote buttons",
            Column::with_children(rows).spacing(6).into(),
        ));
        let selected = self.selected_key.clone();
        content.push(card(
            "Selected button",
            if let Some(key) = &selected {
                column![
                    text(humanize(key)).size(18),
                    text(
                        device
                            .keys
                            .get(key)
                            .map(|command| format!("Sends {}", humanize(command)))
                            .unwrap_or_else(|| "Nothing assigned".into())
                    )
                    .size(13),
                    action("Clear", Message::ClearKey, self.busy),
                ]
                .spacing(8)
                .into()
            } else {
                text("Choose a button above or in the list.")
                    .size(13)
                    .into()
            },
        ));
        let filter = self.command_filter.to_lowercase();
        let mut palette: Vec<Element<'_, Message>> = Vec::new();
        for command in device.commands.keys() {
            if filter.is_empty() || command.to_lowercase().contains(&filter) {
                let mut item = button(text(humanize(command)).size(13))
                    .width(Fill)
                    .padding([6, 8])
                    .style(button::secondary);
                if selected.is_some() && !self.busy {
                    item = item.on_press(Message::AssignCommand(command.clone()));
                }
                palette.push(item.into());
            }
        }
        content.push(card(
            "Command palette",
            column![
                text_input("Filter commands", &self.command_filter)
                    .on_input(Message::CommandFilter)
                    .width(Fill),
                if palette.is_empty() {
                    let empty: Element<'_, Message> = text("No matching Commands.").size(13).into();
                    empty
                } else {
                    Column::with_children(palette).spacing(4).into()
                },
            ]
            .spacing(8)
            .into(),
        ));
        let mut table: Vec<Element<'_, Message>> = Vec::new();
        for binding in profile::HARMONY_300.key_layout {
            let assigned = device.keys.get(binding.name);
            let inherited = routed_target(&self.model, binding.name, &device.name);
            let value = inherited
                .as_ref()
                .map(|target| format!("inherited from {target}"))
                .or_else(|| assigned.cloned())
                .unwrap_or_else(|| "—".into());
            table.push(
                row![
                    text(humanize(binding.name))
                        .size(13)
                        .width(Length::Fixed(160.0)),
                    text(value).size(13).font(Font::MONOSPACE),
                    iced::widget::Space::new().width(Length::Fill),
                    button("Choose")
                        .on_press_maybe(
                            (!self.busy && inherited.is_none())
                                .then_some(Message::SelectKey(binding.name.into()))
                        )
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        content.push(card(
            "All buttons",
            Column::with_children(table).spacing(4).into(),
        ));
        Column::with_children(content).spacing(16).into()
    }

    fn view_activities(&self) -> Element<'_, Message> {
        let mut content: Vec<Element<'_, Message>> = vec![text("Watch TV").size(28).into()];
        if let Some(error) = &self.error {
            content.push(error_banner(error, self.advanced));
        }
        let names = self.device_names();
        content.push(card(
            "Watch TV setup",
            column![
                text("Which device changes channels?").size(12),
                pick_list(
                    names.clone(),
                    Some(self.model.activity.channel_device.clone()),
                    Message::ActivityChannel
                ),
            ]
            .spacing(8)
            .into(),
        ));
        let mut steps: Vec<Element<'_, Message>> = Vec::new();
        for (index, step) in self.model.activity.steps.iter().enumerate() {
            steps.push(
                row![
                    pick_list(names.clone(), Some(step.device.clone()), move |device| {
                        Message::StepDevice(index, device)
                    }),
                    text_input("power", step.power.as_deref().unwrap_or(""))
                        .on_input(move |value| Message::StepPower(index, value))
                        .width(Length::Fixed(130.0)),
                    text_input("input", step.input.as_deref().unwrap_or(""))
                        .on_input(move |value| Message::StepInput(index, value))
                        .width(Length::Fixed(130.0)),
                    text_input("ms", &step.delay_ms.to_string())
                        .on_input(move |value| Message::StepDelay(index, value))
                        .width(Length::Fixed(80.0)),
                    button("Delete")
                        .on_press_maybe((!self.busy).then_some(Message::DeleteStep(index)))
                        .style(button::danger),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        content.push(card(
            "Devices to switch on",
            if steps.is_empty() {
                text("No devices added yet.").size(13).into()
            } else {
                Column::with_children(steps).spacing(6).into()
            },
        ));
        content.push(card(
            "Add a device",
            column![
                pick_list(
                    names,
                    Some(self.activity_device.clone()),
                    Message::ActivityDevice
                ),
                row![
                    text_input("Power command", &self.activity_power)
                        .on_input(Message::ActivityPower)
                        .width(Fill),
                    text_input("Input command", &self.activity_input)
                        .on_input(Message::ActivityInput)
                        .width(Fill),
                    text_input("Wait (ms)", &self.activity_delay)
                        .on_input(Message::ActivityDelay)
                        .width(Length::Fixed(100.0)),
                    action("Add device", Message::AddStep, self.busy),
                ]
                .spacing(8),
            ]
            .spacing(8)
            .into(),
        ));
        Column::with_children(content).spacing(16).into()
    }

    fn view_favourites(&self) -> Element<'_, Message> {
        let mut content: Vec<Element<'_, Message>> = vec![text("Favourites").size(28).into()];
        if let Some(error) = &self.error {
            content.push(error_banner(error, self.advanced));
        }
        let capacity = profile::skin_profile(&profile::HARMONY_300, self.model.skin)
            .map(|skin| skin.favourite_capacity)
            .unwrap_or(0);
        content.push(
            text(format!(
                "Your remote has {capacity} favourite channel button{}.",
                if capacity == 1 { "" } else { "s" }
            ))
            .size(13)
            .into(),
        );
        let mut favourites: Vec<Element<'_, Message>> = Vec::new();
        for (index, favourite) in self.model.favourites.iter().enumerate() {
            favourites.push(
                row![
                    text(format!("Button {}", favourite.key))
                        .size(13)
                        .width(Length::Fixed(70.0)),
                    text_input("channel", &favourite.channel)
                        .on_input(move |value| Message::FavouriteChannel(index, value))
                        .width(Length::Fixed(140.0)),
                    button("Remove")
                        .on_press_maybe((!self.busy).then_some(Message::DeleteFavourite(index)))
                        .style(button::danger),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            );
        }
        content.push(card(
            "Your favourites",
            if favourites.is_empty() {
                text("No favourite channels yet.").size(13).into()
            } else {
                Column::with_children(favourites).spacing(6).into()
            },
        ));
        content.push(card(
            "Add a favourite",
            row![
                text_input("Channel number", &self.new_favourite_channel)
                    .on_input(Message::NewFavouriteChannel)
                    .width(Fill),
                action("Add", Message::AddFavourite, self.busy),
            ]
            .spacing(8)
            .into(),
        ));
        Column::with_children(content).spacing(16).into()
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let mut content: Vec<Element<'_, Message>> = vec![text("Settings").size(28).into()];
        if let Some(error) = &self.error {
            content.push(error_banner(error, self.advanced));
        }
        content.push(card(
            "Display",
            column![
                text("Changes are saved automatically.").size(13),
                checkbox(self.advanced)
                    .label("Show technical details")
                    .on_toggle(Message::ToggleAdvanced),
            ]
            .spacing(8)
            .into(),
        ));
        if self.advanced {
            content.push(card(
                "Remote setup",
                column![
                    data_line("Data directory", &self.data_dir.display().to_string()),
                    data_line("Model", &self.model_path.display().to_string()),
                    data_line("Container", &self.container_path.display().to_string()),
                    data_line("Backups", &self.backups_dir.display().to_string()),
                    row![
                        text("Skin").size(13).width(Length::Fixed(140.0)),
                        pick_list(vec![78u8, 79], Some(self.model.skin), Message::SkinChanged),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        action("Save now", Message::Save, self.busy),
                        action("Reload", Message::Reload, self.busy),
                    ]
                    .spacing(8),
                ]
                .spacing(8)
                .into(),
            ));
            content.push(card(
                "Configuration file",
                column![
                    text(
                        "The active setup is ordinary Model JSON. Export it for the CLI, or import a setup prepared elsewhere."
                    )
                    .size(12),
                    text_input("Path to Model JSON", &self.model_transfer_path)
                        .on_input(Message::TransferPath)
                        .width(Fill),
                    row![
                        action("Export Model", Message::ExportModel, self.busy),
                        action("Import Model", Message::ImportModel, self.busy),
                    ]
                    .spacing(8),
                ]
                .spacing(8)
                .into(),
            ));
        }
        content.push(card(
            "About",
            column![
                text("Remote Rebirth — offline setup for your Harmony 300").size(13),
                text("Works without an internet connection.").size(12),
            ]
            .spacing(6)
            .into(),
        ));
        Column::with_children(content).spacing(16).into()
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

fn data_line(label: &str, value: &str) -> Element<'static, Message> {
    row![
        text(label.to_string()).size(12).width(Length::Fixed(120.0)),
        text(value.to_string()).size(12).font(Font::MONOSPACE),
    ]
    .spacing(8)
    .into()
}

fn action(label: &'static str, message: Message, disabled: bool) -> Element<'static, Message> {
    button(text(label).size(14))
        .on_press_maybe((!disabled).then_some(message))
        .style(button::primary)
        .padding([8, 14])
        .into()
}

fn friendly_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("no harmony remote found") || lower.contains("no remote found") {
        "No Harmony remote found. Connect it with the USB cable and try again.".into()
    } else if lower.contains("multiple harmony remotes") {
        "More than one Harmony remote is connected. Disconnect all but one.".into()
    } else if lower.contains("declares 0 devices") {
        "Add at least one device before checking the setup.".into()
    } else if lower.contains("does not bind key") {
        "Choose the volume or channel buttons for the target device first.".into()
    } else if lower.contains("which routing") {
        "This button is controlled by another device. Change the volume or channel control first."
            .into()
    } else if lower.contains("backup validation") || lower.contains("unverified backup") {
        "That backup is damaged or incomplete. Choose another backup.".into()
    } else if lower.contains("current configuration does not match backup") {
        "The remote has changed since the backup. Save a new backup, then try again.".into()
    } else if lower.contains("read-back mismatch") {
        "The remote did not store the update correctly. Restore your backup.".into()
    } else if lower.contains("container is for skin") {
        "This setup is for a different remote version. Use the connected remote's version, then check the setup again.".into()
    } else if lower.contains("container is") || lower.contains("container checksum") {
        "The new setup is not valid. Check the devices and buttons.".into()
    } else if lower.contains("link_packet_length") {
        "This remote is not responding correctly. Reconnect it and try again.".into()
    } else {
        error.to_string()
    }
}

fn error_banner(error: &str, advanced: bool) -> Element<'_, Message> {
    let mut body = column![
        row![
            text(friendly_error(error)).size(13),
            iced::widget::Space::new().width(Length::Fill),
            button(text(if advanced {
                "Hide details"
            } else {
                "Technical details"
            }))
            .on_press(Message::ToggleAdvanced(!advanced))
            .style(button::secondary),
            button("Dismiss")
                .on_press(Message::DismissError)
                .style(button::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    ]
    .spacing(4);
    if advanced {
        body = body.push(text(error.to_string()).size(11).font(Font::MONOSPACE));
    }
    container(body)
        .padding(10)
        .width(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(
                theme.extended_palette().danger.weak.color,
            )),
            border: Border {
                radius: 8.0.into(),
                width: 1.0,
                color: theme.extended_palette().danger.base.color,
            },
            ..Default::default()
        })
        .into()
}

fn card<'a>(title: impl ToString, content: Element<'a, Message>) -> Element<'a, Message> {
    container(
        column![
            text(title.to_string()).size(16).font(Font::MONOSPACE),
            content
        ]
        .spacing(10),
    )
    .padding(16)
    .width(Fill)
    .style(card_style)
    .into()
}

fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.06),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 4.0,
        },
        ..Default::default()
    }
}

fn sidebar_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border {
            width: 1.0,
            color: palette.background.strong.color,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn footer_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border {
            width: 1.0,
            color: palette.background.strong.color,
            ..Default::default()
        },
        ..Default::default()
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

fn backup_detail(path: &Path, advanced: bool) -> String {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(".json");
    let Ok(bytes) = std::fs::read(PathBuf::from(sidecar)) else {
        return "Saved backup".into();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return "Saved backup".into();
    };
    let validation = value["validation"].as_str().unwrap_or("unknown");
    if !advanced {
        return if validation == "ok" {
            "Saved backup · valid".into()
        } else {
            "Saved backup · damaged".into()
        };
    }
    format!(
        "{} · Skin {} · {} · {} B",
        value["model"].as_str().unwrap_or("unknown"),
        value["skin"]
            .as_u64()
            .map(|skin| skin.to_string())
            .unwrap_or_else(|| "?".into()),
        validation,
        value["size"].as_u64().unwrap_or(0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
