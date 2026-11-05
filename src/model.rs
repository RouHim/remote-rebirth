use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::profile::{self, ModelProfile};

pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub schema_version: u8,
    pub profile: String,
    pub skin: u8,
    pub devices: Vec<Device>,
    pub activity: Activity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub favourites: Vec<Favourite>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub name: String,
    pub device_key: u8,
    pub commands: BTreeMap<String, Command>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub source: IrSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sends: Option<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IrSource {
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub channel_device: String,
    pub steps: Vec<ActivityStep>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityStep {
    pub device: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default = "default_delay_ms")]
    pub delay_ms: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Favourite {
    pub key: u8,
    pub channel: String,
}

fn default_delay_ms() -> u32 {
    500
}

pub const CANONICAL_KEYS: &[&str] = &[
    "power",
    "volume_up",
    "volume_down",
    "volume_mute",
    "channel_up",
    "channel_down",
    "number_0",
    "number_1",
    "number_2",
    "number_3",
    "number_4",
    "number_5",
    "number_6",
    "number_7",
    "number_8",
    "number_9",
    "direction_up",
    "direction_down",
    "direction_left",
    "direction_right",
    "select",
    "menu",
    "exit",
    "info",
    "guide",
    "play",
    "pause",
    "stop",
    "rewind",
    "fast_forward",
    "skip_back",
    "skip_forward",
    "record",
    "red",
    "green",
    "yellow",
    "blue",
    "list",
    "live",
    "prev_channel",
    "clear",
    "enter",
    "input",
    "teletext",
];

pub fn load(path: &Path) -> Result<Model, Error> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        Error::Validation(format!("invalid model JSON {}: {error}", path.display()))
    })
}

pub fn validate(model: &Model, profile: &ModelProfile) -> Result<(), Error> {
    if model.schema_version != SCHEMA_VERSION {
        return Err(Error::Validation(format!(
            "unsupported model schema_version {}; expected {SCHEMA_VERSION}",
            model.schema_version
        )));
    }
    if model.profile != profile.id {
        return Err(Error::Validation(format!(
            "model profile '{}' does not match the active profile '{}'",
            model.profile, profile.id
        )));
    }
    let skin = profile::skin_profile(profile, model.skin).ok_or_else(|| {
        Error::Validation(format!(
            "skin {} is not accepted by profile '{}'",
            model.skin, profile.id
        ))
    })?;
    if model.devices.is_empty() || model.devices.len() > profile.max_devices {
        return Err(Error::Validation(format!(
            "model declares {} Devices; profile '{}' accepts 1 to {}",
            model.devices.len(),
            profile.id,
            profile.max_devices
        )));
    }

    let mut device_names = BTreeSet::new();
    let mut device_keys = BTreeSet::new();
    let mut referenced: BTreeSet<(String, String)> = BTreeSet::new();

    for device in &model.devices {
        if device.name.trim().is_empty() {
            return Err(Error::Validation("a Device name is empty".into()));
        }
        if !device_names.insert(device.name.clone()) {
            return Err(Error::Validation(format!(
                "Device '{}' is declared twice",
                device.name
            )));
        }
        if device.device_key == 0 || usize::from(device.device_key) > profile.max_devices {
            return Err(Error::Validation(format!(
                "Device '{}' uses device key {}; profile '{}' accepts 1 to {}",
                device.name, device.device_key, profile.id, profile.max_devices
            )));
        }
        if !device_keys.insert(device.device_key) {
            return Err(Error::Validation(format!(
                "device key {} is assigned to more than one Device",
                device.device_key
            )));
        }
        if device.commands.is_empty() {
            return Err(Error::Validation(format!(
                "Device '{}' declares no Commands",
                device.name
            )));
        }
        for (command_name, command) in &device.commands {
            if command_name.trim().is_empty() {
                return Err(Error::Validation(format!(
                    "Device '{}' has a Command with an empty name",
                    device.name
                )));
            }
            validate_source(&device.name, command_name, &command.source)?;
            if command.sends == Some(0) {
                return Err(Error::Validation(format!(
                    "Command '{}' on Device '{}' sets sends to 0",
                    command_name, device.name
                )));
            }
        }
        for (key_name, command_name) in &device.keys {
            if !CANONICAL_KEYS.contains(&key_name.as_str()) {
                return Err(Error::Validation(format!(
                    "Device '{}' binds unknown key '{key_name}'",
                    device.name
                )));
            }
            if !device.commands.contains_key(command_name) {
                return Err(Error::Validation(format!(
                    "Device '{}' binds key '{key_name}' to missing Command '{command_name}'",
                    device.name
                )));
            }
            referenced.insert((device.name.clone(), command_name.clone()));
        }
    }

    let channel_device = model
        .devices
        .iter()
        .find(|device| device.name == model.activity.channel_device)
        .ok_or_else(|| {
            Error::Validation(format!(
                "Watch TV Activity names missing channel Device '{}'",
                model.activity.channel_device
            ))
        })?;
    let _ = channel_device;
    if model.activity.steps.is_empty() {
        return Err(Error::Validation("Watch TV Activity has no steps".into()));
    }
    for step in &model.activity.steps {
        let device = model
            .devices
            .iter()
            .find(|device| device.name == step.device)
            .ok_or_else(|| {
                Error::Validation(format!(
                    "Watch TV Activity names missing Device '{}'",
                    step.device
                ))
            })?;
        if step.power.is_none() && step.input.is_none() {
            return Err(Error::Validation(format!(
                "Watch TV Activity step for Device '{}' has neither a power nor an input Command",
                step.device
            )));
        }
        for command_name in [step.power.as_ref(), step.input.as_ref()]
            .into_iter()
            .flatten()
        {
            if !device.commands.contains_key(command_name) {
                return Err(Error::Validation(format!(
                    "Watch TV Activity step for Device '{}' names missing Command '{command_name}'",
                    step.device
                )));
            }
            referenced.insert((device.name.clone(), command_name.clone()));
        }
    }

    for device in &model.devices {
        for command_name in device.commands.keys() {
            if !referenced.contains(&(device.name.clone(), command_name.clone())) {
                return Err(Error::Validation(format!(
                    "Command '{}' on Device '{}' is not referenced by keys or the Watch TV Activity",
                    command_name, device.name
                )));
            }
        }
    }

    if model.favourites.len() > skin.favourite_capacity {
        return Err(Error::Validation(format!(
            "model declares {} Favourites; skin {} accepts {}",
            model.favourites.len(),
            skin.skin,
            skin.favourite_capacity
        )));
    }
    let mut favourite_keys = BTreeSet::new();
    for favourite in &model.favourites {
        if favourite.key == 0 || usize::from(favourite.key) > skin.favourite_capacity {
            return Err(Error::Validation(format!(
                "Favourite key {} is outside 1 to {} for skin {}",
                favourite.key, skin.favourite_capacity, skin.skin
            )));
        }
        if !favourite_keys.insert(favourite.key) {
            return Err(Error::Validation(format!(
                "Favourite key {} is assigned more than once",
                favourite.key
            )));
        }
        if !valid_channel(&favourite.channel) {
            return Err(Error::Validation(format!(
                "Favourite '{}' is not a 1 to 5 digit channel without a leading zero",
                favourite.channel
            )));
        }
    }

    Ok(())
}

fn validate_source(device: &str, command: &str, source: &IrSource) -> Result<(), Error> {
    let context = format!("Command '{command}' on Device '{device}'");
    match source.format.as_str() {
        "pronto" => {
            if source.hex.is_some() == source.file.is_some() {
                return Err(Error::Validation(format!(
                    "{context} must set exactly one of hex or file for Pronto"
                )));
            }
            if let Some(hex) = &source.hex
                && hex.trim().is_empty()
            {
                return Err(Error::Validation(format!("{context} has empty Pronto hex")));
            }
        }
        "lirc" => {
            if source.file.is_none() || source.hex.is_some() {
                return Err(Error::Validation(format!(
                    "{context} must set file and not hex for LIRC"
                )));
            }
            if source.remote.as_deref().unwrap_or_default().is_empty()
                || source.button.as_deref().unwrap_or_default().is_empty()
            {
                return Err(Error::Validation(format!(
                    "{context} must set remote and button for LIRC"
                )));
            }
        }
        "flipper" => {
            if source.file.is_none() || source.hex.is_some() {
                return Err(Error::Validation(format!(
                    "{context} must set file and not hex for Flipper"
                )));
            }
            if source.command.as_deref().unwrap_or_default().is_empty() {
                return Err(Error::Validation(format!(
                    "{context} must set command for Flipper"
                )));
            }
        }
        other => {
            return Err(Error::Validation(format!(
                "{context} uses unknown IR Source format '{other}'"
            )));
        }
    }
    Ok(())
}

fn valid_channel(channel: &str) -> bool {
    let bytes = channel.as_bytes();
    if bytes.is_empty() || bytes.len() > 5 || !bytes.iter().all(u8::is_ascii_digit) {
        return false;
    }
    bytes.len() == 1 || bytes[0] != b'0'
}
