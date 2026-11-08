//! What each message does, grouped by the page it belongs to.
//!
//! [`App::update`](crate::app::App::update) is a thin router: the busy guard lives there, the
//! work lives here. Every handler keeps the arms it owns verbatim, so a message moves by moving
//! one arm.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use iced::Task;

use remote_rebirth_core::gui_api::{self, RestoreRequest, WriteRequest};
use remote_rebirth_core::model::{
    ActivityStep, Command, Favourite, IrSource, Routing, SCHEMA_VERSION,
};
use remote_rebirth_core::{HidBackend, Progress, SystemClock, profile};

use crate::app::App;
use crate::message::{Message, OpDone, Page, PreparedKind};
use crate::operation::{check_cancel, operation_task};
use crate::support::{error_text, non_empty, routed_target, scan_library, step_label};

impl App {
    /// Page navigation, the connected remote, and the backup offered for restore.
    pub(crate) fn on_remote(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Page(page) => {
                self.ui.page = page;
                self.save_gui_state();
                Task::none()
            }
            Message::RestorePath(path) => {
                if self.ui.restore_path != path
                    && self.operation.prepared_kind == Some(PreparedKind::Restore)
                {
                    self.operation.prepared_kind = None;
                    self.operation.last_prepare = None;
                }
                self.ui.restore_path = path;
                Task::none()
            }
            Message::UseBackup(path) => {
                let selected = path.display().to_string();
                if self.ui.restore_path != selected
                    && self.operation.prepared_kind == Some(PreparedKind::Restore)
                {
                    self.operation.prepared_kind = None;
                    self.operation.last_prepare = None;
                }
                self.ui.restore_path = selected;
                self.ui.page = Page::Remote;
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
                if self.project.backup_path.as_deref() == Some(path.as_path()) {
                    self.project.backup_path = None;
                    self.operation.prepared_kind = None;
                    self.operation.last_prepare = None;
                }
                if Path::new(self.ui.restore_path.trim()) == path.as_path() {
                    self.operation.prepared_kind = None;
                    self.operation.last_prepare = None;
                }
                self.refresh_backups();
                Task::none()
            }
            Message::UseDetectedSkin(skin) => {
                if profile::skin_profile(&profile::HARMONY_300, skin).is_some() {
                    self.project.model.skin = skin;
                    self.ui.error = None;
                    self.persist();
                    self.operation.status =
                        "Setup switched to the connected remote's version.".into();
                } else {
                    self.ui.error = Some(
                        "The connected remote's version is not supported by this setup.".into(),
                    );
                }
                Task::none()
            }
            Message::Reboot(reboot) => {
                self.ui.reboot = reboot;
                self.operation.prepared_kind = None;
                self.operation.last_prepare = None;
                self.save_gui_state();
                Task::none()
            }
            Message::DismissError => {
                self.ui.error = None;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Starting an operation, its progress, cancellation, and its result.
    pub(crate) fn on_operation(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Detect => {
                self.begin("Detecting the Remote");
                let cancel = self.operation.cancel.clone();
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
                let cancel = self.operation.cancel.clone();
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
                let model = self.project.model.clone();
                let model_path = self.project.model_path.clone();
                let container = self.project.container_path.clone();
                let cancel = self.operation.cancel.clone();
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
                let model = self.project.model.clone();
                let model_path = self.project.model_path.clone();
                let container = self.project.container_path.clone();
                let backup = self.backup_path_for("pre-write");
                let reboot = self.ui.reboot;
                let state_dir = self.project.state_dir.clone();
                let cancel = self.operation.cancel.clone();
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
                        WriteRequest {
                            state_dir: &state_dir,
                            container_path: &container,
                            backup_path: &backup,
                            commit: false,
                            no_reboot: !reboot,
                        },
                        progress,
                    )
                    .map_err(error_text)?;
                    Ok(OpDone::Prepared(report))
                })
            }
            Message::CommitWrite if self.operation.prepared_kind == Some(PreparedKind::Write) => {
                let reboot = self.ui.reboot;
                let state_dir = self.project.state_dir.clone();
                match (
                    self.project.container_path.clone(),
                    self.project.backup_path.clone(),
                ) {
                    (container, Some(backup)) => {
                        self.begin("Writing to the Remote");
                        self.operation.committing = true;
                        operation_task(move |progress| {
                            let mut backend = HidBackend::new();
                            let clock = SystemClock;
                            gui_api::write(
                                &mut backend,
                                &clock,
                                WriteRequest {
                                    state_dir: &state_dir,
                                    container_path: &container,
                                    backup_path: &backup,
                                    commit: true,
                                    no_reboot: !reboot,
                                },
                                progress,
                            )
                            .map(OpDone::Committed)
                            .map_err(error_text)
                        })
                    }
                    _ => {
                        self.ui.error = Some("Prepare a Write first.".into());
                        Task::none()
                    }
                }
            }
            Message::CommitWrite => {
                self.ui.error = Some("Prepare a Write first.".into());
                Task::none()
            }
            Message::PrepareRestore => {
                let path = self.restore_path();
                let reboot = self.ui.reboot;
                let state_dir = self.project.state_dir.clone();
                let cancel = self.operation.cancel.clone();
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
                                RestoreRequest {
                                    state_dir: &state_dir,
                                    backup_path: &path,
                                    commit: false,
                                    no_reboot: !reboot,
                                },
                                progress,
                            )
                            .map(OpDone::RestorePrepared)
                            .map_err(error_text)
                        })
                    }
                    Err(error) => {
                        self.ui.error = Some(error);
                        Task::none()
                    }
                }
            }
            Message::CommitRestore
                if self.operation.prepared_kind == Some(PreparedKind::Restore) =>
            {
                let path = self.restore_path();
                let reboot = self.ui.reboot;
                let state_dir = self.project.state_dir.clone();
                let cancel = self.operation.cancel.clone();
                match path {
                    Ok(path) => {
                        self.begin("Restoring the Remote");
                        self.operation.committing = true;
                        operation_task(move |progress| {
                            check_cancel(&cancel)?;
                            let mut backend = HidBackend::new();
                            let clock = SystemClock;
                            gui_api::restore(
                                &mut backend,
                                &clock,
                                RestoreRequest {
                                    state_dir: &state_dir,
                                    backup_path: &path,
                                    commit: true,
                                    no_reboot: !reboot,
                                },
                                progress,
                            )
                            .map(OpDone::Restored)
                            .map_err(error_text)
                        })
                    }
                    Err(error) => {
                        self.ui.error = Some(error);
                        Task::none()
                    }
                }
            }
            Message::CommitRestore => {
                self.ui.error = Some("Prepare a Restore first.".into());
                Task::none()
            }
            Message::Progress(progress) => {
                match progress {
                    Progress::Step(step) => {
                        self.operation.progress = Some((step_label(step).to_string(), 0.0));
                    }
                    Progress::Written { bytes, total } => {
                        let fraction = if total == 0 {
                            0.0
                        } else {
                            bytes as f32 / total as f32
                        };
                        self.operation.progress =
                            Some((format!("writing {bytes}/{total}"), fraction));
                    }
                }
                Task::none()
            }
            Message::Done(result) => {
                self.operation.busy = false;
                self.operation.committing = false;
                self.operation.progress = None;
                match result {
                    Ok(done) => self.finish(done),
                    Err(error) => {
                        self.ui.error = Some(error);
                        self.operation.status = "Operation failed.".into();
                    }
                }
                Task::none()
            }
            Message::Cancel => {
                self.operation.cancel.store(true, Ordering::Relaxed);
                self.operation.status = "Cancelling after the current step...".into();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Device slots, their names, keys, and command lists.
    pub(crate) fn on_devices(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectDevice(index) => {
                if index < self.project.model.devices.len() {
                    self.ui.selected_device = index;
                    self.ui.selected_key = None;
                    self.ui.activity_device = self.project.model.devices[index].name.clone();
                    self.sync_device_name_draft();
                    self.save_gui_state();
                }
                Task::none()
            }
            Message::NewDeviceName(name) => {
                self.ui.new_device_name = name;
                Task::none()
            }
            Message::NewDeviceKey(key) => {
                self.ui.new_device_key = key;
                Task::none()
            }
            Message::AddDevice => {
                self.add_device();
                Task::none()
            }
            Message::DeviceName(index, name) => {
                if self.project.model.devices.get(index).is_none() {
                    return Task::none();
                }
                self.ui.device_name_draft = Some((index, name.clone()));
                let duplicate = self
                    .project
                    .model
                    .devices
                    .iter()
                    .enumerate()
                    .any(|(other, device)| other != index && device.name == name);
                if duplicate {
                    self.ui.error = Some(format!("Device '{name}' already exists."));
                    return Task::none();
                }
                if let Some(device) = self.project.model.devices.get_mut(index) {
                    let old = device.name.clone();
                    if old == name {
                        return Task::none();
                    }
                    device.name = name.clone();
                    for step in &mut self.project.model.activity.steps {
                        if step.device == old {
                            step.device = name.clone();
                        }
                    }
                    if self.project.model.activity.channel_device == old {
                        self.project.model.activity.channel_device = name.clone();
                    }
                    if let Some(routing) = &mut self.project.model.routing {
                        if routing.volume.as_deref() == Some(old.as_str()) {
                            routing.volume = Some(name.clone());
                        }
                        if routing.channel.as_deref() == Some(old.as_str()) {
                            routing.channel = Some(name.clone());
                        }
                    }
                    if self.ui.activity_device == old {
                        self.ui.activity_device = name.clone();
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::DeviceKey(index, key) => {
                if self
                    .project
                    .model
                    .devices
                    .iter()
                    .enumerate()
                    .all(|(other, device)| other == index || device.device_key != key)
                {
                    if let Some(device) = self.project.model.devices.get_mut(index) {
                        device.device_key = key;
                    }
                    self.persist();
                } else {
                    self.ui.error = Some("That device button is already used.".into());
                }
                Task::none()
            }
            Message::DeleteDevice(index) => {
                if index < self.project.model.devices.len() {
                    let removed = self.project.model.devices.remove(index);
                    self.project
                        .model
                        .activity
                        .steps
                        .retain(|step| step.device != removed.name);
                    if self.project.model.activity.channel_device == removed.name {
                        self.project.model.activity.channel_device = self
                            .project
                            .model
                            .devices
                            .first()
                            .map(|device| device.name.clone())
                            .unwrap_or_default();
                    }
                    if let Some(routing) = &mut self.project.model.routing {
                        if routing.volume.as_deref() == Some(removed.name.as_str()) {
                            routing.volume = None;
                        }
                        if routing.channel.as_deref() == Some(removed.name.as_str()) {
                            routing.channel = None;
                        }
                    }
                    if let Some(routing) = &self.project.model.routing
                        && routing.volume.is_none()
                        && routing.channel.is_none()
                    {
                        self.project.model.routing = None;
                    }
                    if !self
                        .project
                        .model
                        .devices
                        .iter()
                        .any(|device| device.name == self.ui.activity_device)
                    {
                        self.ui.activity_device = self
                            .project
                            .model
                            .devices
                            .first()
                            .map(|device| device.name.clone())
                            .unwrap_or_default();
                    }
                    if index < self.ui.selected_device {
                        self.ui.selected_device -= 1;
                    }
                    self.ui.selected_device = self
                        .ui
                        .selected_device
                        .min(self.project.model.devices.len().saturating_sub(1));
                    self.ui.selected_key = None;
                    self.sync_device_name_draft();
                    self.persist();
                }
                Task::none()
            }
            Message::NewCommandName(name) => {
                self.ui.new_command.name = name;
                Task::none()
            }
            Message::NewCommandFormat(format) => {
                self.ui.new_command.format = format;
                Task::none()
            }
            Message::NewCommandFile(file) => {
                self.ui.new_command.file = file;
                Task::none()
            }
            Message::NewCommandRemote(remote) => {
                self.ui.new_command.remote = remote;
                Task::none()
            }
            Message::NewCommandButton(button) => {
                self.ui.new_command.button = button;
                Task::none()
            }
            Message::NewCommandHex(hex) => {
                self.ui.new_command.hex = hex;
                Task::none()
            }
            Message::AddCommand => {
                self.add_command();
                Task::none()
            }
            Message::DeleteCommand(name) => {
                if let Some(device) = self.project.model.devices.get_mut(self.ui.selected_device) {
                    device.commands.remove(&name);
                    device.keys.retain(|_, command| command != &name);
                    for step in &mut self.project.model.activity.steps {
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
            _ => Task::none(),
        }
    }

    /// Key selection, command assignment, and volume/channel routing.
    pub(crate) fn on_keymap(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectKey(key) => {
                self.ui.selected_key = Some(key);
                self.save_gui_state();
                Task::none()
            }
            Message::ClearKey => {
                if let Some(key) = self.ui.selected_key.clone() {
                    if let Some(device) =
                        self.project.model.devices.get_mut(self.ui.selected_device)
                    {
                        device.keys.remove(&key);
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::CommandFilter(filter) => {
                self.ui.command_filter = filter;
                Task::none()
            }
            Message::AssignFirstCommand => self.assign_first_command(),
            Message::AssignCommand(command) => {
                if let Some(key) = self.ui.selected_key.clone() {
                    let routed = self
                        .project
                        .model
                        .devices
                        .get(self.ui.selected_device)
                        .and_then(|device| routed_target(&self.project.model, &key, &device.name));
                    if let Some(target) = routed {
                        self.ui.error = Some(format!(
                            "Key '{key}' is routed to '{target}' and cannot be bound here."
                        ));
                        return Task::none();
                    }
                    if let Some(device) =
                        self.project.model.devices.get_mut(self.ui.selected_device)
                    {
                        device.keys.insert(key, command);
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::RoutingVolume(target) => {
                if target.is_some() {
                    self.project.model.schema_version = SCHEMA_VERSION;
                }
                let routing = self
                    .project
                    .model
                    .routing
                    .get_or_insert_with(Routing::default);
                routing.volume = target;
                if routing.volume.is_none() && routing.channel.is_none() {
                    self.project.model.routing = None;
                }
                self.persist();
                Task::none()
            }
            Message::RoutingChannel(target) => {
                if target.is_some() {
                    self.project.model.schema_version = SCHEMA_VERSION;
                }
                let routing = self
                    .project
                    .model
                    .routing
                    .get_or_insert_with(Routing::default);
                routing.channel = target;
                if routing.volume.is_none() && routing.channel.is_none() {
                    self.project.model.routing = None;
                }
                self.persist();
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// The ordered steps of the Watch TV activity.
    pub(crate) fn on_activities(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ActivityChannel(name) => {
                self.project.model.activity.channel_device = name;
                self.persist();
                Task::none()
            }
            Message::ActivityDevice(name) => {
                self.ui.activity_device = name;
                self.ui.activity_power.clear();
                self.ui.activity_input.clear();
                Task::none()
            }
            Message::ActivityPower(value) => {
                self.ui.activity_power = value;
                Task::none()
            }
            Message::ActivityInput(value) => {
                self.ui.activity_input = value;
                Task::none()
            }
            Message::ActivityDelay(value) => {
                self.ui.activity_delay = value;
                Task::none()
            }
            Message::StepDevice(index, name) => {
                if let Some(step) = self.project.model.activity.steps.get_mut(index) {
                    step.device = name;
                }
                self.persist();
                Task::none()
            }
            Message::StepPower(index, power) => {
                if let Some(step) = self.project.model.activity.steps.get_mut(index) {
                    step.power = non_empty(power);
                }
                self.persist();
                Task::none()
            }
            Message::StepInput(index, input) => {
                if let Some(step) = self.project.model.activity.steps.get_mut(index) {
                    step.input = non_empty(input);
                }
                self.persist();
                Task::none()
            }
            Message::StepDelay(index, delay) => {
                if let Some(step) = self.project.model.activity.steps.get_mut(index) {
                    step.delay_ms = delay.parse().unwrap_or(500);
                }
                self.persist();
                Task::none()
            }
            Message::AddStep => {
                let device = self
                    .project
                    .model
                    .devices
                    .iter()
                    .find(|device| device.name == self.ui.activity_device)
                    .or_else(|| self.project.model.devices.first())
                    .map(|device| device.name.clone())
                    .unwrap_or_default();
                if !device.is_empty() {
                    self.project.model.activity.steps.push(ActivityStep {
                        device,
                        power: non_empty(self.ui.activity_power.clone()),
                        input: non_empty(self.ui.activity_input.clone()),
                        delay_ms: self.ui.activity_delay.parse().unwrap_or(500),
                    });
                    if self.project.model.activity.channel_device.is_empty() {
                        self.project.model.activity.channel_device =
                            self.project.model.activity.steps[0].device.clone();
                    }
                    self.persist();
                }
                Task::none()
            }
            Message::DeleteStep(index) => {
                if index < self.project.model.activity.steps.len() {
                    self.project.model.activity.steps.remove(index);
                    self.persist();
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// The favourite channel buttons.
    pub(crate) fn on_favourites(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NewFavouriteChannel(channel) => {
                self.ui.new_favourite_channel = channel;
                Task::none()
            }
            Message::AddFavourite => {
                let capacity =
                    profile::skin_profile(&profile::HARMONY_300, self.project.model.skin)
                        .map(|skin| skin.favourite_capacity)
                        .unwrap_or(0);
                let key = (1..=capacity).find(|key| {
                    !self
                        .project
                        .model
                        .favourites
                        .iter()
                        .any(|favourite| usize::from(favourite.key) == *key)
                });
                if self.ui.new_favourite_channel.trim().is_empty() {
                    self.ui.error = Some("Enter a channel number.".into());
                } else if let Some(key) = key {
                    self.project.model.favourites.push(Favourite {
                        key: key as u8,
                        channel: self.ui.new_favourite_channel.trim().to_string(),
                    });
                    self.ui.new_favourite_channel.clear();
                    self.persist();
                } else {
                    self.ui.error = Some(format!(
                        "Skin {} holds at most {capacity} Favourites.",
                        self.project.model.skin
                    ));
                }
                Task::none()
            }
            Message::FavouriteChannel(index, channel) => {
                if let Some(favourite) = self.project.model.favourites.get_mut(index) {
                    favourite.channel = channel;
                }
                self.persist();
                Task::none()
            }
            Message::DeleteFavourite(index) => {
                if index < self.project.model.favourites.len() {
                    self.project.model.favourites.remove(index);
                    for (position, favourite) in
                        self.project.model.favourites.iter_mut().enumerate()
                    {
                        favourite.key = position as u8 + 1;
                    }
                    self.persist();
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// The local IR Library browser.
    pub(crate) fn on_library(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LibraryPath(path) => {
                self.library.dir = path;
                Task::none()
            }
            Message::ScanLibrary => {
                match scan_library(Path::new(&self.library.dir)) {
                    Ok(devices) => {
                        self.library.status = if devices.is_empty() {
                            "No .ir files with named commands found.".to_string()
                        } else {
                            "Scan complete.".to_string()
                        };
                        self.library.selected = (!devices.is_empty()).then_some(0);
                        self.library.devices = devices;
                        self.library.error = None;
                    }
                    Err(error) => {
                        self.library.error = Some(error);
                    }
                }
                Task::none()
            }
            Message::LibrarySearch(search) => {
                self.library.search = search;
                Task::none()
            }
            Message::SelectLibraryDevice(index) => {
                if index < self.library.devices.len() {
                    self.library.selected = Some(index);
                }
                Task::none()
            }
            Message::AddLibraryDevice => {
                let Some(source) = self
                    .library
                    .selected
                    .and_then(|index| self.library.devices.get(index))
                    .cloned()
                else {
                    self.library.error = Some("Choose a library device first.".into());
                    return Task::none();
                };
                if self.project.model.devices.is_empty() {
                    self.library.error = Some("Choose a device on My Devices first.".into());
                    return Task::none();
                }
                let device_index = self
                    .ui
                    .selected_device
                    .min(self.project.model.devices.len().saturating_sub(1));
                let target_name = self.project.model.devices[device_index].name.clone();
                let mut added = 0;
                let mut skipped = 0;
                for name in &source.commands {
                    let target = &mut self.project.model.devices[device_index];
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
                                button: None,
                                command: Some(name.clone()),
                            },
                            sends: None,
                        },
                    );
                    added += 1;
                }
                self.persist();
                self.library.status = format!(
                    "Added {added} command(s) to {target_name}; {skipped} already present."
                );
                self.library.error = None;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Display preferences and the project files.
    pub(crate) fn on_settings(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SkinChanged(skin) => {
                self.project.model.skin = skin;
                self.persist();
                Task::none()
            }
            Message::Save => {
                self.persist();
                if self.project.saved {
                    self.operation.status = format!("Saved {}", self.project.model_path.display());
                } else {
                    self.operation.status = "The setup could not be saved.".to_string();
                }
                Task::none()
            }
            Message::Reload => {
                if let Ok(model) = gui_api::load_model(&self.project.model_path) {
                    self.project.model = model;
                    self.ui.selected_device = 0;
                    self.ui.selected_key = None;
                    self.ui.activity_device = self
                        .project
                        .model
                        .devices
                        .first()
                        .map(|device| device.name.clone())
                        .unwrap_or_default();
                    self.operation.container_size = None;
                    self.operation.prepared_kind = None;
                    self.operation.last_prepare = None;
                    self.sync_device_name_draft();
                    self.operation.status =
                        format!("Reloaded {}", self.project.model_path.display());
                } else {
                    self.ui.error = Some("The saved setup cannot be read.".into());
                }
                Task::none()
            }
            Message::TransferPath(path) => {
                self.project.model_transfer_path = path;
                Task::none()
            }
            Message::ExportModel => {
                let path = PathBuf::from(self.project.model_transfer_path.trim());
                if path.as_os_str().is_empty() {
                    self.ui.error = Some("Choose an export path.".into());
                } else {
                    match gui_api::save_model(&path, &self.project.model) {
                        Ok(()) => self.operation.status = format!("Exported {}", path.display()),
                        Err(error) => self.ui.error = Some(error_text(error)),
                    }
                }
                Task::none()
            }
            Message::ImportModel => {
                let path = PathBuf::from(self.project.model_transfer_path.trim());
                match gui_api::load_model(&path) {
                    Ok(model) => match gui_api::validate_model(&model) {
                        Ok(()) => {
                            self.project.model = model;
                            self.ui.selected_device = 0;
                            self.ui.selected_key = None;
                            self.ui.activity_device = self
                                .project
                                .model
                                .devices
                                .first()
                                .map(|device| device.name.clone())
                                .unwrap_or_default();
                            self.operation.container_size = None;
                            self.sync_device_name_draft();
                            self.persist();
                            self.operation.status = format!("Imported {}", path.display());
                        }
                        Err(error) => self.ui.error = Some(error_text(error)),
                    },
                    Err(error) => self.ui.error = Some(error_text(error)),
                }
                Task::none()
            }
            Message::ToggleAdvanced(advanced) => {
                self.ui.advanced = advanced;
                self.save_gui_state();
                Task::none()
            }
            _ => Task::none(),
        }
    }
}
