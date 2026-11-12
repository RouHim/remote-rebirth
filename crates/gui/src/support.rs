//! Small conversions shared by the App and the pages: paths, labels, and text.
//!
//! Nothing here talks to the remote; these are the pure helpers that turn Model values into the
//! words the window shows.

use std::path::{Path, PathBuf};

use remote_rebirth_core::gui_api;
use remote_rebirth_core::model::{IrSource, Model, Routing};
use remote_rebirth_core::profile;

use crate::message::{CommandForm, LibraryDevice};

/// Where the window keeps its project when no other directory is given.
pub(crate) fn app_data_dir() -> PathBuf {
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

/// Turn the "Add a command" form into the IR source the Model stores.
pub(crate) fn form_source(form: &CommandForm) -> Result<IrSource, String> {
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

pub(crate) fn device_key_labels() -> Vec<String> {
    profile::HARMONY_300
        .device_key_labels
        .iter()
        .map(|label| label.to_string())
        .collect()
}

pub(crate) fn device_key_label(key: u8) -> String {
    profile::HARMONY_300
        .device_key_labels
        .get(usize::from(key).saturating_sub(1))
        .copied()
        .unwrap_or("?")
        .to_string()
}

pub(crate) fn device_key_from_label(label: &str) -> u8 {
    profile::HARMONY_300
        .device_key_labels
        .iter()
        .position(|candidate| *candidate == label)
        .map(|index| index as u8 + 1)
        .unwrap_or(1)
}

pub(crate) fn command_formats() -> Vec<String> {
    vec![
        "Pronto code".into(),
        "LIRC file".into(),
        "Flipper file".into(),
    ]
}

pub(crate) fn command_format_label(format: &str) -> String {
    match format {
        "lirc" => "LIRC file",
        "flipper" => "Flipper file",
        _ => "Pronto code",
    }
    .to_string()
}

pub(crate) fn command_format_from_label(label: &str) -> String {
    match label {
        "LIRC file" => "lirc",
        "Flipper file" => "flipper",
        _ => "pronto",
    }
    .to_string()
}

/// The device a routed key actually reaches, when that is not the device itself.
pub(crate) fn routed_target(model: &Model, key: &str, device: &str) -> Option<String> {
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

pub(crate) fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn none_if_placeholder(value: String) -> Option<String> {
    if value == "Each device itself" {
        None
    } else {
        Some(value)
    }
}

pub(crate) fn humanize(value: &str) -> String {
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

pub(crate) fn short_digest(digest: &str) -> String {
    digest.chars().take(12).collect()
}

/// Renders an optional protocol number for technical details without `Some(...)`.
pub(crate) fn plain_number(value: Option<u8>) -> String {
    value.map_or_else(|| "?".into(), |value| value.to_string())
}

pub(crate) fn error_text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub(crate) fn step_label(step: &str) -> &str {
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

/// Newest first, so the backup list shows the recovery point closest to now.
///
/// Names carry `<prefix>-<nanoseconds>-<sequence>.bin`; sorting the whole path would group by
/// prefix before timestamp (`backup-` sorts after `pre-write-`), so the embedded stamp decides.
pub(crate) fn list_backups(directory: &Path) -> Vec<PathBuf> {
    let mut backups: Vec<PathBuf> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("bin"))
        .collect();
    backups.sort_by(|left, right| {
        backup_order(right)
            .cmp(&backup_order(left))
            .then_with(|| right.cmp(left))
    });
    backups
}

/// The `(timestamp, sequence)` a Backup name embeds, or zero for a foreign file.
fn backup_order(path: &Path) -> (u128, u64) {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| {
            let mut parts = stem.rsplit('-');
            let sequence = parts.next()?.parse().ok()?;
            let timestamp = parts.next()?.parse().ok()?;
            Some((timestamp, sequence))
        })
        .unwrap_or_default()
}

/// Index the `.ir` files below `root`, newest scan wins.
pub(crate) fn scan_library(root: &Path) -> Result<Vec<LibraryDevice>, String> {
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
            // The core decides which commands a file really offers, so the Library can never
            // list one that Compose would refuse.
            let Ok(commands) = gui_api::list_flipper_commands(&path) else {
                continue;
            };
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
