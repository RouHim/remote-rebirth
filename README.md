# Remote Rebirth

Offline configuration tool for the Logitech Harmony 300.

Remote Rebirth identifies a Harmony 300 over USB, backs up its Configuration, Composes a new
Container from a declarative JSON model plus locally imported IR Sources, and Writes or Restores the
Container with a checksum check, a Read-Back, and a Journal. It performs no network access and does
not depend on a Logitech account or service.

## Build

Requires Rust 1.90 or newer (edition 2024 plus the MSRV of the pinned dependencies).

```sh
cargo build --release
```

The CLI binary is `target/release/remote-rebirth`; the GUI binary is
`target/release/remote-rebirth-gui`. The transport uses hidapi's libusb backend, whose build script
compiles C and probes libusb through `pkg-config`, so the build needs a C toolchain plus libusb's
development files (`base-devel`, `libusb` and `pkg-config` on Arch; `build-essential`,
`libusb-1.0-0-dev` and `pkg-config` on Debian/Ubuntu).

## Testing

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Tests use the fake Remote and a fixed clock, so they need no hardware and touch no real
configuration. Most of the suite lives inline in `crates/*/src` (sibling `*_tests.rs` modules);
`crates/cli/tests/cli.rs` holds the CLI smoke tests, which run the built binary.

## USB permissions

The CLI needs access to the Remote's USB device node. Install the provided udev rule:

```sh
sudo install -m 0644 contrib/99-remote-rebirth.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

The rule covers the USB device and its `hidraw` node and grants a dedicated `remote-rebirth` group
access, so it works for local and headless/SSH sessions without handing raw HID write access to every
administrator:

```sh
sudo groupadd -f remote-rebirth
sudo usermod -aG remote-rebirth $USER
```

Log out and back in so the new group membership takes effect; an SSH session needs the same
re-login. Members of `wheel` may substitute `GROUP="wheel"` in the rule if they prefer, at the cost
that every administrator then gets that access. The transport opens the USB device through libusb;
the `hidraw` node is not required, and the rule's hidraw line is harmless.
Reconnect the Remote after installing the rule; for an already-connected unit, a software re-authorise
(`echo 0/1` to `/sys/bus/usb/devices/<port>/authorized`) re-enumerates it without physical access.

After a committed `write` or `restore` reboots the Remote, wait for USB re-enumeration before the next
command; the device node changes.

## Desktop GUI

`cargo run -p remote-rebirth-gui` starts the desktop application. It needs a Wayland or X11 session;
when no GPU/GL driver is available, iced falls back to software rendering. The CLI is the headless
surface. The GUI links the same core as the
CLI, never shells out, and contains no network client.

The GUI manages an active Model at `$XDG_DATA_HOME/remote-rebirth/model.json` (falling back to
`$HOME/.local/share/remote-rebirth/model.json`, then to the relative `.remote-rebirth/model.json`
when neither variable is set), keeps automatic and manual Backups under
`backups/`, and drives the same verified Write and Restore path as the CLI. It covers Detect,
Backup, Device and Command editing, the Key Map with volume/channel Routing, Prepare/Commit Write,
Prepare/Commit Restore, the Watch TV Activity and Favourites, and an IR Library browser that scans
a local folder of Flipper `.ir` files and imports their commands. The pinned Flipper-IRDB download,
drag-and-drop assignment, and deeper Favourites and Activity polish are the next slice.

## Core API

The public surface of `remote-rebirth-core` — the CLI seam, the typed GUI operations, the Model and
Profile types, the transport traits and the exit-code taxonomy — is documented in
[`docs/core-api.md`](docs/core-api.md), with a source citation for every item.

## Commands

```sh
remote-rebirth detect [--json]
remote-rebirth backup --out <path> [--overwrite] [--json] [--print-schema]
remote-rebirth inspect --file <path> [--json] [--print-schema]
remote-rebirth compose --model <model.json> --out <container.bin> [--overwrite] [--json]
                        [--print-schema] [--print-resolved-schema]
remote-rebirth write --container <path> --backup <path> [--commit] [--no-reboot] [--json]
remote-rebirth restore --backup <path> [--commit] [--no-reboot] [--json]
```

`write` and `restore` default to a Dry Run. Only `--commit` opens `/cfg/usercfg` for writing.

`--print-schema` prints the embedded JSON Schema for that command's data without requiring its
other inputs: `backup` prints the Backup sidecar schema, `compose` prints the model schema, and
`compose --print-resolved-schema` prints the resolved-model sidecar schema.

## Model schema versions

Model v2 is the current format. `compose --print-schema` prints the v2 schema, `compose` reads v2
Models, and the resolved-model sidecar it writes declares `schema_version: 2`. Model v1 is still
accepted on input — the validator allows `schema_version` 1 through 2 (`MIN_SCHEMA_VERSION`) — as
long as a v1 Model declares no `routing`. The v1 input schema stays in the repository at
`crates/core/schema/model-v1.json`; no CLI flag prints it. The v1 resolved-model schema was removed
because no source, test or flag read it. New Models should be v2.

The published v2 schema is profile-agnostic: it allows up to 8 devices and any non-negative delay,
while the active Model Profile may be stricter (Harmony 300: 4 devices). Compose rounds a positive
delay up to 100 ms steps and clamps it to 1–255 tenths of a second (0.1–25.5 s); `delay_ms` 0 emits
no wait at all.

## Safety model

- `backup` writes a Container plus a JSON sidecar containing the identity, size, SHA-256 digest, UTC
  timestamp, and validation status. It refuses to overwrite unless `--overwrite` is given.
- `write --commit` requires a verified Backup whose model and Skin match the connected Remote, and
  verifies that the Remote's current Configuration equals that Backup or is empty.
- `restore --commit` refuses a Backup whose sidecar validation is not `ok`; this is stricter than the
  literal specification, deliberately, so a damaged capture is never replayed over a working
  Configuration.
- The committed Write follows the file protocol: filesystem reset, open for writing, chunks of at most
  50 data packets, checksum check, flush only on `m`, close, Read-Back, reboot.
- A Read-Back mismatch exits non-zero, skips the reboot, and names the Backup and the
  `restore --backup <path> --commit` recovery command.
- Failures Journal the last acknowledged step under `$XDG_STATE_HOME/remote-rebirth/journal.jsonl`.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Internal or unexpected error |
| 2 | Usage error |
| 3 | Device error: not found, USB failure, unsupported model |
| 4 | Validation error: model, limits, Container size or checksum, Backup mismatch |
| 5 | Protocol error: refusal, malformed packet, short read, checksum check not `m` |
| 6 | Verification error: Read-Back mismatch |

JSON goes to stdout; diagnostics go to stderr. Write and Restore print their stages and acknowledged
byte counts to stderr when stderr is a terminal, so a piped transcript stays clean.

## Supported hardware and platforms

Linux x86_64 is the only supported platform: the transport is hidapi's libusb backend and Detect
reads `/sys/bus/usb/devices` directly. `remote-rebirth-core` refuses to compile on other targets
with a `compile_error!` (`crates/core/src/lib.rs` tail).

| Device | Product | Status |
| --- | --- | --- |
| Harmony 300 | `046d:c124` | Supported. Skins 78 and 79, Architecture 16, measured on firmware 1.4.0. A different firmware major.minor is accepted but logs a warning that the write sequence is unverified on it (`crates/core/src/profile.rs:331`, `crates/core/src/write.rs:381`). |
| Harmony 350 | `046d:c124` | Recognised and refused: shares the product id but its skin 104 has no profile. |
| Harmony 200 | `046d:c125` | Recognised and refused: Detect reads its identity, then reports it as unsupported. |
| Other Logitech devices | any | Reported unsupported; no configuration is written. |

## IR Sources

The model references user-supplied IR Sources. Timing-bearing inputs (Pronto `0000`/`0100`, LIRC
`raw_codes`, Flipper `raw`) become class-1 pulse records directly. Pronto shorthand, LIRC `codes`, and
Flipper `parsed` inputs use the six supported protocol encoders: NEC, RC5, Sony SIRC, Kaseikyo,
Samsung32, and JVC. Unsupported protocols and flags fail explicitly.

No Logitech catalogue data is shipped or embedded. All IR data comes from user-supplied open-format
files.

## Hardware verification

The reference documents no write, commit, or device-control sequence ever sent to a Harmony 300; the
sequence is inferred from Logitech's client and one firmware reading. Physical acceptance on a real
Harmony 300 is the release gate for the write path.

## License

GPL-3.0-or-later. See `LICENSE`.
