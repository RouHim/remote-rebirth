//! `compose --model <model.json> --out <container.bin> [--overwrite] [--json]`.
//!
//! The model is loaded and validated against the Harmony 300 Model Profile, every Command's IR Source is
//! imported, and [`crate::compose_build::build`] lays the Container out from nothing. Beside the
//! Container this writes `<out>.model.json`, the resolved model: every Command with its IR Source, the
//! provenance the import recorded and a compact summary of the IR Record it became.

use std::collections::BTreeMap;
#[cfg(feature = "cli")]
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

/// The model input schema, embedded so `compose --print-schema` needs no installed files.
#[cfg(feature = "cli")]
pub const MODEL_SCHEMA: &str = include_str!("../schema/model-v2.json");
/// The resolved-model sidecar schema, embedded for `compose --print-resolved-schema`.
#[cfg(feature = "cli")]
pub const RESOLVED_SCHEMA: &str = include_str!("../schema/resolved-model-v2.json");

/// A composed Container without any CLI formatting.
#[derive(Debug, Clone)]
pub struct ComposeReport {
    pub container: PathBuf,
    pub resolved_model: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub devices: usize,
    pub favourites: usize,
    pub unused: Vec<(String, String)>,
}

pub fn create(
    clock: &dyn Clock,
    model_path: &Path,
    out: &Path,
    overwrite: bool,
) -> Result<ComposeReport, Error> {
    let span = tracing::info_span!("compose", model = %model_path.display(), out = %out.display());
    let _entered = span.enter();

    let model = model::load(model_path)?;
    let profile = crate::profile::by_id(&model.profile).ok_or_else(|| {
        Error::Validation(format!(
            "model names unknown Model Profile '{}'",
            model.profile
        ))
    })?;
    model::validate(&model, profile)?;
    let unused = model::unused_commands(&model);

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
    crate::output::write_file(out, &composed.bytes)?;
    crate::output::write_file(&sidecar, &serde_json::to_vec_pretty(&resolved)?)?;
    tracing::info!(
        size = composed.bytes.len(),
        devices = composed.devices.len(),
        favourites = composed.favourites,
        unused = unused.len(),
        resolved_model = %sidecar.display(),
        "composed Container"
    );

    Ok(ComposeReport {
        container: out.to_path_buf(),
        resolved_model: sidecar,
        size: composed.bytes.len() as u64,
        sha256: container::sha256_hex(&composed.bytes),
        devices: composed.devices.len(),
        favourites: composed.favourites,
        unused,
    })
}

#[cfg(feature = "cli")]
pub fn run(
    clock: &dyn Clock,
    model_path: &Path,
    out: &Path,
    overwrite: bool,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let report = create(clock, model_path, out, overwrite)?;
    if json {
        let output = ComposeOutput {
            schema_version: 1,
            container: report.container.display().to_string(),
            resolved_model: report.resolved_model.display().to_string(),
            size: report.size,
            sha256: report.sha256,
            devices: report.devices,
            favourites: report.favourites,
        };
        serde_json::to_writer_pretty(&mut *stdout, &output)?;
        writeln!(stdout)?;
        return Ok(());
    }

    writeln!(
        stdout,
        "Wrote Container {} ({} bytes, sha256 {})",
        report.container.display(),
        report.size,
        report.sha256
    )?;
    writeln!(
        stdout,
        "Wrote resolved model {}",
        report.resolved_model.display()
    )?;
    if !report.unused.is_empty() {
        tracing::warn!(
            unused = report.unused.len(),
            "unreferenced Commands stay in the Model and are omitted from the Container"
        );
    }
    Ok(())
}

fn model_directory(model_path: &Path) -> &Path {
    match model_path.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory,
        _ => Path::new("."),
    }
}

/// Import every referenced Command's IR Source, in Device order and Command name order. Commands
/// the Model keeps but no key or Activity sends are left as `None`; they stay in the Model and are
/// omitted from the Container and resolved sidecar.
fn import_commands(
    model: &Model,
    model_dir: &Path,
) -> Result<Vec<Vec<Option<ImportedCommand>>>, Error> {
    let referenced = model::referenced_commands(model);
    let mut imports = Vec::with_capacity(model.devices.len());
    for device in &model.devices {
        let mut device_imports = Vec::with_capacity(device.commands.len());
        for (name, command) in &device.commands {
            if !referenced.contains(&(device.name.clone(), name.clone())) {
                device_imports.push(None);
                continue;
            }
            let context = format!("Command '{name}' on Device '{}'", device.name);
            let source = ir_source(&command.source, &context)?;
            let imported = ir::import(&source, model_dir, command.sends)
                .map_err(|error| Error::Validation(format!("{context}: {error}")))?;
            device_imports.push(Some(imported));
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

#[cfg(feature = "cli")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    routing: Option<model::Routing>,
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
        imports: &[Vec<Option<ImportedCommand>>],
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
            routing: model.routing.clone(),
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
        imports: &[Option<ImportedCommand>],
    ) -> Self {
        let commands = device
            .commands
            .iter()
            .zip(imports)
            .zip(&placement.record_indices)
            .filter_map(|(((name, command), imported), index)| {
                let imported = imported.as_ref()?;
                let index = index.as_ref()?;
                let record = RecordSummary::of(placement.ir_group, *index, &imported.record);
                Some((
                    name.clone(),
                    ResolvedCommand {
                        source: command.source.clone(),
                        provenance: imported.provenance.clone(),
                        record,
                    },
                ))
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
