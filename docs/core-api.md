# Core public API

`remote-rebirth-core` is the only crate that talks to the Remote or touches the Container format.
The CLI adapter and the GUI both depend on it; neither reimplements a safety gate. The crate is
Linux-only by construction (`crates/core/src/lib.rs:80`).

Items are documented here, not in rustdoc bodies. Item-level rustdoc is tracked as follow-up; this
page is the current contract and every claim cites its source.

## Feature gates

`remote-rebirth-core` has one feature, `cli`, and it is on by default (`crates/core/Cargo.toml`).
It adds the `run` adapter and its clap dependency. The GUI depends on the core with
`default-features = false` (`crates/gui/Cargo.toml:18`), so the GUI surface is everything below
except `run` and `Runtime`. The CLI turns the feature on (`crates/cli/Cargo.toml:14`).

## The CLI seam

```rust
pub fn run(args: &[OsString], runtime: &mut Runtime<'_>) -> Exit
```

`run` owns parsing, dispatch, validation, codecs, protocol sequencing, artifacts, JSON, and exit
codes (`crates/core/src/cli.rs:102`). It writes JSON to `runtime.stdout` and diagnostics to
`runtime.stderr`. It never panics on a usage error: it maps clap failures to the usage exit code.

`Runtime` is the injected environment (`crates/core/src/runtime.rs:7`):

```rust
pub struct Runtime<'a> {
    pub backend: &'a mut dyn RemoteBackend,
    pub clock: &'a dyn Clock,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    pub state_dir: PathBuf,
}
```

Tests pass the fake Remote and a fixed clock through this struct; nothing else is a test seam.

## GUI operations

The GUI calls typed functions instead of parsing CLI output (`crates/core/src/gui_api.rs`).

```rust
pub const DETECT_SCHEMA_VERSION: u8 = 1;
pub fn detect(backend: &mut dyn RemoteBackend) -> Result<DetectReport, Error>
pub fn backup(backend: &mut dyn RemoteBackend, clock: &dyn Clock, out: &Path, overwrite: bool)
    -> Result<BackupReport, Error>
pub fn compose(clock: &dyn Clock, model_path: &Path, out: &Path, overwrite: bool)
    -> Result<ComposeReport, Error>
pub fn write(backend: &mut dyn RemoteBackend, clock: &dyn Clock, request: WriteRequest<'_>,
             progress: &mut dyn FnMut(Progress)) -> Result<WriteReport, Error>
pub fn restore(backend: &mut dyn RemoteBackend, clock: &dyn Clock, request: RestoreRequest<'_>,
               progress: &mut dyn FnMut(Progress)) -> Result<WriteReport, Error>
pub fn load_model(path: &Path) -> Result<Model, Error>
pub fn save_model(path: &Path, model: &Model) -> Result<(), Error>
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error>
pub fn backup_sidecar(backup: &Path) -> PathBuf
pub fn backup_summary(sidecar: &Path) -> Result<BackupSummary, Error>
pub fn list_flipper_commands(path: &Path) -> Result<Vec<String>, Error>
pub fn validate_model(model: &Model) -> Result<(), Error>
pub fn blank_model(skin: u8) -> Model
```

| Item | Source | Behaviour |
| --- | --- | --- |
| `DETECT_SCHEMA_VERSION` | `crates/core/src/gui_api.rs:29` | The detect report's schema version, stamped into every JSON output. |
| `detect` | `crates/core/src/gui_api.rs:74` | Reads identity without writing. An unsupported model comes back as `DetectReport { supported: false, .. }`, not an error. |
| `backup` | `crates/core/src/gui_api.rs:116` | Writes a Container plus its sidecar; refuses to overwrite unless `overwrite`. |
| `compose` | `crates/core/src/gui_api.rs:125` | Compiles a Model into a Container and its resolved-model sidecar. |
| `write` | `crates/core/src/gui_api.rs:134` | Prepare or commit a Container through `WriteRequest`. A commit requires a verified Backup and journals under `state_dir`. |
| `restore` | `crates/core/src/gui_api.rs:143` | Prepare or commit a Backup through `RestoreRequest`. A commit refuses a sidecar whose validation is not `ok`. |
| `load_model` | `crates/core/src/gui_api.rs:152` | Reads and parses a Model JSON file. |
| `save_model` | `crates/core/src/gui_api.rs:156` | Serialises pretty JSON through `write_atomic`, so a failed save never truncates the previous Model. |
| `write_atomic` | `crates/core/src/gui_api.rs:162` | Writes through a unique sibling temporary file and renames it into place. A symlink at `path` is refused rather than followed, and a failure leaves the old contents untouched. |
| `backup_sidecar` | `crates/core/src/gui_api.rs:167` | The sidecar path belonging to a Backup, so the GUI never rebuilds the naming convention itself. |
| `backup_summary` | `crates/core/src/gui_api.rs:203` | Reads one Backup sidecar into a typed `BackupSummary`. A missing or malformed sidecar is a validation error. |
| `list_flipper_commands` | `crates/core/src/gui_api.rs:232` | The command names one local Flipper `.ir` file offers, validated the same way an import validates them. |
| `validate_model` | `crates/core/src/gui_api.rs:244` | Resolves `model.profile` through `profile::by_id`, then calls `model::validate`. An unknown profile is a validation error. |
| `blank_model` | `crates/core/src/gui_api.rs:255` | A starting Model for editing. Deliberately invalid until a Device and an Activity exist. |

The request structs carry only paths and flags:

```rust
pub struct WriteRequest<'a> {   // crates/core/src/write.rs:24
    pub state_dir: &'a Path,
    pub container_path: &'a Path,
    pub backup_path: &'a Path,
    pub commit: bool,
    pub no_reboot: bool,
}
pub struct RestoreRequest<'a> { // crates/core/src/restore.rs:18
    pub state_dir: &'a Path,
    pub backup_path: &'a Path,
    pub commit: bool,
    pub no_reboot: bool,
}
```

`DetectReport` (`crates/core/src/gui_api.rs:33`) carries `schema_version`, `supported`, `model`,
`family`, `profile`, `architecture`, `skin`, `firmware_version`, `hardware_version`, `guid`, `serial`
and `usb`. `UsbIdentity` (`crates/core/src/gui_api.rs:49`) carries the descriptor's vendor id,
product id, `bcd_device`, product, serial, bus and address. `BackupSummary`
(`crates/core/src/gui_api.rs:173`) carries `backup`, `sidecar`, `model`, `skin`, `architecture`,
`firmware_version`, `size`, `sha256`, `timestamp` and `validation` as read from a sidecar.
`BackupReport`, `ComposeReport` and `WriteReport` are re-exported as `BackupResult`, `ComposeResult`
and `WriteResult` (`crates/core/src/gui_api.rs:24`–`26`):

| Report | Source | Fields |
| --- | --- | --- |
| `BackupReport` | `crates/core/src/backup.rs:21` | `container`, `sidecar`, `model`, `skin`, `architecture`, `firmware_version`, `guid`, `serial`, `size`, `sha256`, `timestamp`, `validation`. |
| `ComposeReport` | `crates/core/src/compose.rs:32` | `container`, `resolved_model`, `size`, `sha256`, `devices`, `favourites`, `unused`. |
| `WriteReport` | `crates/core/src/write.rs:34` | `operation`, `commit`, `container`, `backup`, `size`, `backup_sha256`, `bytes`, `chunks`, `data_packets`, `reboot`, `journal`. |

## Model

Everything in `remote_rebirth_core::model` (`crates/core/src/model.rs`). All serde structs use
`deny_unknown_fields`.

```rust
pub const SCHEMA_VERSION: u8 = 2;      // emitted and printed
pub const MIN_SCHEMA_VERSION: u8 = 1;  // oldest accepted on input
pub const CANONICAL_KEYS: &[&str];     // the key names a Device may bind
pub fn load(path: &Path) -> Result<Model, Error>
pub fn validate(model: &Model, profile: &ModelProfile) -> Result<(), Error>
pub fn referenced_commands(model: &Model) -> BTreeSet<(String, String)>
pub fn unused_commands(model: &Model) -> Vec<(String, String)>
```

| Item | Source | Notes |
| --- | --- | --- |
| `SCHEMA_VERSION` | `crates/core/src/model.rs:9` | v2. `compose --print-schema` prints the v2 schema. |
| `MIN_SCHEMA_VERSION` | `crates/core/src/model.rs:10` | v1. v1 Models are still accepted on input. |
| `Model` | `crates/core/src/model.rs:14` | `schema_version`, `profile`, `skin`, `devices`, `routing`, `activity`, `favourites`. |
| `Routing` | `crates/core/src/model.rs:30` | `volume` and `channel`, each an optional Device name. `VOLUME_KEYS` and `CHANNEL_KEYS` list the routed keys. |
| `Device` | `crates/core/src/model.rs:44` | `name`, `device_key`, `commands`, `keys`. |
| `Command` | `crates/core/src/model.rs:54` | `source` and an optional `sends` code. |
| `IrSource` | `crates/core/src/model.rs:62` | `format` plus the optional `file`, `hex`, `remote`, `button` and `command` selectors. |
| `Activity` | `crates/core/src/model.rs:78` | One Watch TV activity: `channel_device` and ordered `steps`. |
| `ActivityStep` | `crates/core/src/model.rs:85` | `device`, optional `power` and `input`, `delay_ms` defaulting to 500. |
| `Favourite` | `crates/core/src/model.rs:97` | Positional `key` and `channel`. Capacity comes from the Skin. |
| `CANONICAL_KEYS` | `crates/core/src/model.rs:106` | The canonical key names. A Model may bind only these. |
| `load` | `crates/core/src/model.rs:153` | Reads and parses a Model JSON file. |
| `validate` | `crates/core/src/model.rs:160` | The full Model check: schema version, limits, bindings, Activity, Favourites, Routing. |
| `referenced_commands` | `crates/core/src/model.rs:338` | The `(device, command)` pairs a Container needs: key bindings, Activity steps, routed targets. |
| `unused_commands` | `crates/core/src/model.rs:357` | Commands that stay in the Model but are omitted from the Container. |

## Profiles

Everything in `remote_rebirth_core::profile` (`crates/core/src/profile.rs`). Profiles carry the
per-model and per-Skin facts the codecs need; no catalogue data is shipped.

| Item | Source | Notes |
| --- | --- | --- |
| `KeyBinding` | `crates/core/src/profile.rs:5` | A canonical key name and its scan code. |
| `SkinProfile` | `crates/core/src/profile.rs:11` | Favourite capacity and scans, IR group count, label. |
| `ModelProfile` | `crates/core/src/profile.rs:22` | Identity, architecture, firmware, skins, limits, key layout, Device Key scans and labels. |
| `HARMONY_300_SKINS` | `crates/core/src/profile.rs:41` | Skin 78 (5 favourites) and skin 79 (4 favourites). |
| `HARMONY_300_DEVICE_KEY_SCANS` | `crates/core/src/profile.rs:63` | `[5, 1, 7, 4]`, top left to bottom right. |
| `HARMONY_300_DEVICE_KEY_LABELS` | `crates/core/src/profile.rs:66` | `TV`, `Cable/Sat`, `DVD`, `VCR/Aux`. |
| `family` | `crates/core/src/profile.rs:70` | Maps `bcdDevice` to `Harmony 300` or `Harmony 350`. |
| `HARMONY_300_KEYS` | `crates/core/src/profile.rs:81` | The Harmony 300 keypad: every canonical key except `live`. |
| `HARMONY_300` | `crates/core/src/profile.rs:256` | The only production Model Profile. |
| `all` | `crates/core/src/profile.rs:310` | Iterates the production profiles. |
| `by_id` | `crates/core/src/profile.rs:314` | Resolves a Model's `profile` string. |
| `for_identity` | `crates/core/src/profile.rs:318` | Resolves architecture and skin to a profile. |
| `firmware_matches` | `crates/core/src/profile.rs:331` | Compares a detected firmware string with the profile's. |
| `for_skin` | `crates/core/src/profile.rs:345` | Resolves a skin alone to a profile. |
| `recognized_model` | `crates/core/src/profile.rs:349` | Names a recognised but unsupported model. |
| `skin_profile` | `crates/core/src/profile.rs:357` | Looks a Skin up inside a profile. |
| `key_scan` | `crates/core/src/profile.rs:362` | Canonical key name to scan code. |
| `favourite_key` | `crates/core/src/profile.rs:372` | Scan code to positional favourite. |
| `empty_ir_groups` | `crates/core/src/profile.rs:385` | Test builds only. IR group count used to build an empty configuration. |

`HARMONY_350_TEST`, `HARMONY_350_TEST_SKINS` and `empty_ir_groups`
(`crates/core/src/profile.rs:278`, `:287`, `:385`) exist only in test builds: the real Harmony 350's
skin 104 is recognised and refused, the test profile uses skin 105 so that posture stays observable,
and no shipping command builds an empty Container.

## Errors and exit codes

```rust
pub enum Error { Usage(String), Device(String), Validation(String), Protocol(String),
                 Verification(String), Internal(String), Io(std::io::Error),
                 Json(serde_json::Error), Time(String) }
impl Error { pub fn exit_code(&self) -> u8 }
pub struct Exit(pub u8);
```

`Error` is defined at `crates/core/src/error.rs:4`; `exit_code` at `crates/core/src/error.rs:26`;
`Exit` at `crates/core/src/error.rs:39`.

| Variant | Exit | Meaning |
| --- | --- | --- |
| `Usage` | 2 | Bad arguments. |
| `Device` | 3 | Remote not found, USB failure, unsupported model. |
| `Validation` | 4 | Model, limit, Container size or checksum, Backup mismatch. |
| `Protocol` | 5 | Refusal, malformed packet, short read, checksum check not `m`. |
| `Verification` | 6 | Read-Back mismatch. |
| `Internal`, `Io`, `Json`, `Time` | 1 | Unexpected failure. |

Success is `Exit(0)` (`crates/core/src/cli.rs:113`).

## Clock

```rust
pub trait Clock: Send + Sync { fn now(&self) -> OffsetDateTime; }
pub struct SystemClock;
pub fn format_rfc3339(value: &OffsetDateTime) -> Result<String, Error>
```

Declared at `crates/core/src/clock.rs:5`, `crates/core/src/clock.rs:9` and
`crates/core/src/clock.rs:17`. Every operation takes `&dyn Clock` so tests can freeze time; the
sidecar timestamp and the Journal use it.

## Transport

```rust
pub struct RemoteDescriptor { pub bus: u8, pub address: u8, pub vendor_id: u16,
    pub product_id: u16, pub bcd_device: u16, pub product: Option<String>,
    pub serial: Option<String>, pub path: Option<String> }
pub struct RemoteSelector { pub bus: u8, pub address: u8, pub path: Option<String> }
pub struct OpenRead { pub handle: u8, pub size: u32 }

pub trait RemoteBackend {
    fn list(&mut self) -> Result<Vec<RemoteDescriptor>, Error>;
    fn open(&mut self, selector: &RemoteSelector) -> Result<Box<dyn RemoteSession>, Error>;
}

pub trait RemoteSession {
    fn ping(&mut self) -> Result<(), Error>;
    fn reset_filesystem(&mut self) -> Result<(), Error>;
    fn open_read(&mut self, path: &str) -> Result<OpenRead, Error>;
    fn read(&mut self, handle: u8, packets: u16) -> Result<Vec<u8>, Error>;
    fn open_write(&mut self, path: &str, size: u32) -> Result<u8, Error>;
    fn write_chunk(&mut self, handle: u8, data: &[u8], last: bool) -> Result<(), Error>;
    fn check(&mut self, handle: u8, expected: u16, covered_len: u32) -> Result<bool, Error>;
    fn flush(&mut self, handle: u8) -> Result<(), Error>;
    fn close(&mut self, handle: u8) -> Result<(), Error>;
    fn reboot(&mut self) -> Result<(), Error>;
}

pub fn read_file(session: &mut dyn RemoteSession, path: &str) -> Result<Vec<u8>, Error>
```

Declared at `crates/core/src/transport.rs:8`, `:21`, `:28`, `:33`, `:38` and `:51`. `read_file`
opens, reads in 32-packet requests, checks the size against the 256 KiB Container limit, and closes.

`HidBackend` (`crates/core/src/hid.rs:17`) is the only production `RemoteBackend`; construct it with
`HidBackend::new()` (`crates/core/src/hid.rs:20`). The test suite substitutes its own backend, so no
production code carries a test-only branch.

## Progress

```rust
pub enum Progress {
    Step(&'static str),
    Written { bytes: usize, total: usize },
}
```

Declared at `crates/core/src/progress.rs:4`. `write` and `restore` take
`&mut dyn FnMut(Progress)`. The CLI renders steps and byte counts to stderr only when stderr is a
terminal (`crates/core/src/cli.rs:245`); the GUI forwards the events to its worker channel.

## State directory and empty configuration

```rust
pub fn state_dir() -> PathBuf
pub fn emit_empty(skin: u8, built_at: &OffsetDateTime) -> Result<Vec<u8>, Error>
```

`state_dir` (`crates/core/src/paths.rs:15`) prefers `$XDG_STATE_HOME/remote-rebirth`, then
`$HOME/.local/state/remote-rebirth`, then `.remote-rebirth-state`. The Journal lives there.

`emit_empty` (`crates/core/src/empty.rs:38`) builds the reference empty configuration with its
`PTYY` trailer and checksum for skins 78, 79 and 104. It is exported only in test builds
(`#[cfg(test)]`, `crates/core/src/lib.rs:66`), because no CLI or GUI command writes an empty
Container.

`sidecar_path` and `with_suffix` (`crates/core/src/paths.rs:4`, `:9`) are the only other public path
helpers.
