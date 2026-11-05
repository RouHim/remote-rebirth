//! `compose --model <model.json> --out <container.bin> [--overwrite] [--json]`.
//!
//! The model is loaded and validated against the Harmony 300 Model Profile, every Command's IR Source is
//! imported, and [`crate::compose_build::build`] lays the Container out from nothing. Beside the
//! Container this writes `<out>.model.json`, the resolved model: every Command with its IR Source, the
//! provenance the import recorded and a compact summary of the IR Record it became.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::clock::Clock;
use crate::compose_build::{Composed, ComposedDevice};
use crate::container;
use crate::error::Error;
use crate::ir::{self, ImportedCommand, IrRecord};
use crate::model::{self, Model};
use crate::profile::ModelProfile;

pub fn run(
    clock: &dyn Clock,
    model_path: &Path,
    out: &Path,
    overwrite: bool,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let model = model::load(model_path)?;
    let profile = &crate::profile::HARMONY_300;
    model::validate(&model, profile)?;

    let sidecar = sidecar_path(out);
    if !overwrite {
        let mut existing = Vec::new();
        for path in [out, sidecar.as_path()] {
            if path.exists() {
                existing.push(path.display().to_string());
            }
        }
        if !existing.is_empty() {
            return Err(Error::Validation(format!(
                "{} already exists; pass --overwrite to replace it",
                existing.join(" and ")
            )));
        }
    }

    let model_dir = model_directory(model_path);
    let imports = import_commands(&model, model_dir)?;
    let composed = crate::compose_build::build(&model, profile, &imports, &clock.now())?;

    let resolved = ResolvedModel::new(&model, profile, &imports, &composed);
    fs::write(out, &composed.bytes)?;
    fs::write(&sidecar, serde_json::to_vec_pretty(&resolved)?)?;

    let digest = container::sha256_hex(&composed.bytes);
    if json {
        let output = ComposeOutput {
            schema_version: 1,
            container: out.display().to_string(),
            resolved_model: sidecar.display().to_string(),
            size: composed.bytes.len() as u64,
            sha256: digest,
            devices: composed.devices.len(),
            favourites: composed.favourites,
        };
        serde_json::to_writer_pretty(&mut *stdout, &output)?;
        writeln!(stdout)?;
        return Ok(());
    }

    writeln!(
        stdout,
        "Wrote Container {} ({} bytes, sha256 {})",
        out.display(),
        composed.bytes.len(),
        digest
    )?;
    writeln!(stdout, "Wrote resolved model {}", sidecar.display())?;
    Ok(())
}

fn model_directory(model_path: &Path) -> &Path {
    match model_path.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory,
        _ => Path::new("."),
    }
}

/// Import every Command's IR Source, in Device order and Command name order.
fn import_commands(model: &Model, model_dir: &Path) -> Result<Vec<Vec<ImportedCommand>>, Error> {
    let mut imports = Vec::with_capacity(model.devices.len());
    for device in &model.devices {
        let mut device_imports = Vec::with_capacity(device.commands.len());
        for (name, command) in &device.commands {
            let context = format!("Command '{name}' on Device '{}'", device.name);
            let source = ir_source(&command.source, &context)?;
            let imported = ir::import(&source, model_dir, command.sends)
                .map_err(|error| Error::Validation(format!("{context}: {error}")))?;
            device_imports.push(imported);
        }
        imports.push(device_imports);
    }
    Ok(imports)
}

/// The model's IR Source as the importer takes it.
fn ir_source(source: &model::IrSource, context: &str) -> Result<ir::IrSource, Error> {
    let format = match source.format.as_str() {
        "pronto" => ir::IrSourceKind::Pronto,
        "lirc" => ir::IrSourceKind::Lirc,
        "flipper" => ir::IrSourceKind::Flipper,
        other => {
            return Err(Error::Validation(format!(
                "{context} uses unknown IR Source format '{other}'"
            )));
        }
    };
    let file = match &source.file {
        Some(path) => Some(
            path.to_str()
                .ok_or_else(|| {
                    Error::Validation(format!(
                        "{context} names an IR Source path that is not UTF-8"
                    ))
                })?
                .to_string(),
        ),
        None => None,
    };
    Ok(ir::IrSource {
        format,
        file,
        hex: source.hex.clone(),
        remote: source.remote.clone(),
        button: source.button.clone(),
        command: source.command.clone(),
    })
}

fn sidecar_path(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_os_string();
    name.push(".model.json");
    PathBuf::from(name)
}

#[derive(Serialize)]
struct ComposeOutput {
    schema_version: u8,
    container: String,
    resolved_model: String,
    size: u64,
    sha256: String,
    devices: usize,
    favourites: usize,
}

#[derive(Serialize)]
struct ResolvedModel {
    schema_version: u8,
    profile: String,
    skin: u8,
    devices: Vec<ResolvedDevice>,
    activity: model::Activity,
    favourites: Vec<model::Favourite>,
}

#[derive(Serialize)]
struct ResolvedDevice {
    name: String,
    device_key: u8,
    ir_group: u8,
    map_index: u8,
    commands: BTreeMap<String, ResolvedCommand>,
    keys: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct ResolvedCommand {
    source: model::IrSource,
    provenance: ir::Provenance,
    record: RecordSummary,
}

#[derive(Serialize)]
struct RecordSummary {
    group: u8,
    index: u8,
    class: u8,
    carrier_period_ns: u32,
    carrier_on_ns: u32,
    pointer_groups: usize,
    blocks: usize,
    words: usize,
}

impl RecordSummary {
    fn of(group: u8, index: u8, record: &IrRecord) -> Self {
        let mut blocks = 0;
        let mut words = 0;
        for pointer_group in &record.groups {
            for words_in_block in [
                Some(&pointer_group.once),
                pointer_group.held.as_ref(),
                pointer_group.tail.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                blocks += 1;
                words += words_in_block.len();
            }
        }
        Self {
            group,
            index,
            class: record.class,
            carrier_period_ns: record.carrier_period_ns,
            carrier_on_ns: record.carrier_on_ns,
            pointer_groups: record.groups.len(),
            blocks,
            words,
        }
    }
}

impl ResolvedModel {
    fn new(
        model: &Model,
        profile: &ModelProfile,
        imports: &[Vec<ImportedCommand>],
        composed: &Composed,
    ) -> Self {
        let devices = model
            .devices
            .iter()
            .zip(&composed.devices)
            .zip(imports)
            .map(|((device, placement), imports)| ResolvedDevice::new(device, placement, imports))
            .collect();
        Self {
            schema_version: model::SCHEMA_VERSION,
            profile: profile.id.to_string(),
            skin: model.skin,
            devices,
            activity: model.activity.clone(),
            favourites: model.favourites.clone(),
        }
    }
}

impl ResolvedDevice {
    fn new(
        device: &model::Device,
        placement: &ComposedDevice,
        imports: &[ImportedCommand],
    ) -> Self {
        let commands = device
            .commands
            .iter()
            .zip(imports)
            .zip(&placement.record_indices)
            .map(|(((name, command), imported), index)| {
                let record = RecordSummary::of(placement.ir_group, *index, &imported.record);
                (
                    name.clone(),
                    ResolvedCommand {
                        source: command.source.clone(),
                        provenance: imported.provenance.clone(),
                        record,
                    },
                )
            })
            .collect();
        Self {
            name: device.name.clone(),
            device_key: device.device_key,
            ir_group: placement.ir_group,
            map_index: placement.map_index,
            commands,
            keys: device.keys.clone(),
        }
    }
}
