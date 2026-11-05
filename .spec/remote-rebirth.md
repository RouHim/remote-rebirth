# Feature Specification: Remote Rebirth — Harmony 300 Offline CLI

**Created**: 2026-11-05
**Status**: Approved
**Input**: Build a Rust CLI first, GUI later, for flashing a Harmony 300 over USB without internet access.

## Goal

A Linux user with a Logitech Harmony 300 cannot reconfigure it once Logitech's service disappears. This feature is a native Rust CLI that identifies the remote over USB, backs up its configuration, builds a new configuration entirely on the local machine from a declarative JSON model plus locally imported IR codes, and writes it back with verification. It performs no network access and does not depend on any Logitech account or service. The CLI is the v1 surface; a GUI is explicitly out of scope.

## User Scenarios

### Scenario 1 - Back up the remote before any change (P1)

An owner plugs in a Harmony 300 and captures its current configuration plus identifying metadata, so every later operation has a recoverable original.

**Acceptance**

1. Given a connected Harmony 300 with a readable configuration, when `backup` runs with a target path, then a configuration file and a metadata sidecar are written; the sidecar records model, skin, architecture, firmware version, GUID, serial number, byte size, SHA-256 digest and timestamp.
2. Given an existing file at the target path, when `backup` runs without an explicit overwrite decision, then it exits non-zero and leaves the existing file unchanged.
3. Given no connected remote, when `backup` runs, then it exits non-zero with a message naming the missing device and writes no file.

### Scenario 2 - Compose and flash a new configuration offline (P1)

An owner describes devices, key assignments, a Watch TV shortcut and favourite channels in JSON, imports IR codes from local files, and writes the resulting configuration to the remote without any network connection.

**Acceptance**

1. Given a valid model JSON, valid imported IR codes and a connected Harmony 300, when `compose` runs, then it produces a configuration container containing the declared devices on the four device keys, the Watch TV binding and the declared favourites.
2. Given a composed container, when `write --dry-run` runs, then it validates the model, the backup, the container size, the container checksum and the write plan without opening the remote for writing.
3. Given a composed container and a verified backup, when `write --commit` runs, then the remote is written and read back, and the read-back bytes equal the written bytes.
4. Given a machine with networking disabled, when every command in this scenario runs, then all of them succeed.

### Scenario 3 - Inspect an existing configuration (P2)

An owner or maintainer reads a backup as structured JSON to understand what is on the remote, diff two configurations, or verify a composed result.

**Acceptance**

1. Given a valid Harmony 300 configuration file, when `inspect` runs with `--json`, then the output validates against the published JSON schema and lists container metadata, devices with their key placement, key maps, the Watch TV binding, favourite channels and IR records.
2. Given a file that is not a valid architecture-16 container, when `inspect` runs, then it exits non-zero with a specific parse error and produces no partial JSON on stdout.

### Scenario 4 - Restore a previous backup (P1)

An owner returns the remote to a known-good configuration after an experiment or a failed write.

**Acceptance**

1. Given a backup whose model, skin and architecture match the connected remote, when `restore --commit` runs, then the written configuration is read back and equals the backup byte for byte.
2. Given a backup whose model or skin does not match the connected remote, when `restore` runs, then it refuses before opening the remote for writing.

### Scenario 5 - Add a similar model without changing the codec (P2)

A maintainer prepares support for the Harmony 350, which shares the protocol and configuration format, by adding a model profile rather than branching the parser or writer.

**Acceptance**

1. Given a new model profile declaring product ID, accepted skins, maximum devices, maximum IR groups, favourite capacity and key layout, when the profile is registered, then `detect`, `compose` and validation use it without changes to the container parse or emit code.
2. Given the v1 build, when a Harmony 350 is connected, then the CLI reports that the model is recognised but unsupported in this version, and performs no write.

## Functional Requirements

- **FR-001**: `detect` MUST identify the remote by USB vendor ID `0x046D` and product ID `0xC124`, read `/sys/sysinfo`, and report architecture, model, skin, firmware version, hardware version, GUID and serial number.
- **FR-002**: `detect` MUST refuse to proceed with any write when the connected device is not architecture 16 or is not an enabled model profile.
- **FR-003**: `backup` MUST read `/cfg/usercfg` and write the configuration bytes plus a sidecar containing model, skin, architecture, firmware version, GUID, serial number, byte size, SHA-256 digest and UTC timestamp.
- **FR-004**: `backup` MUST NOT overwrite an existing file unless the user supplies a distinct target path or an explicit overwrite flag.
- **FR-005**: `inspect` MUST parse the architecture-16 container (GSPM header, LWJL and PTYY markers, 15 pointer slots) and emit versioned JSON describing container metadata, devices, key maps, the Watch TV binding, favourites and IR records.
- **FR-006**: `compose` MUST accept a versioned JSON model declaring devices, each device's key placement, the Watch TV activity and favourite channels, and MUST reject a model that exceeds the active profile's device, group or favourite limits.
- **FR-007**: `compose` MUST assign each declared device to exactly one of the four device keys and MUST reject duplicate key assignments.
- **FR-008**: `compose` MUST create the IR records required by every declared command, and MUST reject any command whose protocol it cannot encode.
- **FR-009**: IR import MUST accept Pronto hex, LIRC configuration files and Flipper-IRDB files, and MUST record each command's source and provenance in the composed model.
- **FR-010**: The CLI MUST NOT read, ship, embed or require Logitech's device catalogue; all IR data MUST come from user-supplied open-format files.
- **FR-011**: Every configuration built from nothing MUST bind firmware event `0xFF` to the action list called by the `0x30` entry, and the validator MUST reject a configuration that uses IR groups above 3 without that binding.
- **FR-012**: `write` MUST default to a dry run and MUST require an explicit commit flag before opening `/cfg/usercfg` for writing.
- **FR-013**: Before opening for writing, `write` MUST verify that a backup exists, that its model and skin match the connected remote, that the remote's current configuration equals that backup or is empty, and that the container's size and checksum are valid.
- **FR-014**: A committed write MUST follow the file-protocol sequence: reset the file system, open `/cfg/usercfg` for writing with the declared size, write in chunks of at most 50 data packets, request the checksum check, flush only after the check answers `m`, close, and reboot unless disabled.
- **FR-015**: After a committed write, the CLI MUST read the configuration back and compare it byte for byte with the written container; a mismatch MUST produce a non-zero exit and a recovery message naming the backup.
- **FR-016**: On a checksum mismatch, short read, malformed packet, USB disconnect or any protocol refusal, the CLI MUST abort before flush where the sequence permits, MUST journal the last acknowledged step, and MUST exit non-zero.
- **FR-017**: The CLI MUST reject a container larger than the remote's 256 KiB configuration region.
- **FR-018**: The CLI MUST never open firmware paths for writing; firmware writes are out of scope.
- **FR-019**: All commands MUST operate with networking disabled and MUST contain no network client, telemetry or update check.
- **FR-020**: Machine-readable output MUST be available as JSON on stdout, with progress and diagnostics on stderr; exit codes MUST be documented and stable.
- **FR-021**: Model-specific values MUST live in a data-driven model profile (product ID, accepted skins, architecture, maximum devices, maximum IR groups, favourite capacity, key layout, long-press capability); the v1 build MUST enable the Harmony 300 profile only.

## Key Entities

- **Remote**: the connected unit, identified by USB product ID, architecture, skin, firmware version, GUID and serial number.
- **Model Profile**: the declared capabilities and layout for one model family, used by detection, composition and validation.
- **Configuration**: the architecture-16 container written to `/cfg/usercfg`, including its header, slot table, IR records, devices, key maps, activity binding and favourites.
- **Device**: a named appliance assigned to one device key, with a set of named commands.
- **Command / IR Record**: a named infrared command encoded as the pulse record the remote stores.
- **Activity**: the single Watch TV binding that switches declared devices on and selects their inputs.
- **Favourite**: a channel number bound to one of the remote's favourite keys.
- **Backup**: a configuration file plus its metadata sidecar, used as the mandatory recovery point.

## Edge Cases

- A Harmony 350, Harmony 200 or Harmony Touch is connected: report the recognised model, refuse all writes in v1.
- The remote holds no configuration, or a configuration that fails validation: allow backup only if readable; require an explicit flag before composing over an empty or unreadable region.
- The remote's current configuration differs from the recorded backup: refuse the write and require a fresh backup.
- An open reply states a size larger than the bytes actually received: treat the read as short and abort.
- A malformed packet silences the session: close the handle, reopen a fresh session, and report the failure.
- The checksum check does not answer `m`: do not flush; report that the remote may be left erased and name the backup.
- USB disconnects mid-write: journal the last acknowledged step and report the recovery procedure.
- The model declares more devices, groups or favourites than the active profile allows: reject before composing.
- Two devices are assigned to the same key: reject.
- An IR command uses an unsupported protocol: reject with the protocol name and the command name.
- The composed container exceeds 256 KiB: reject.
- Skins 78 (US) and 79 (EMEA) are both accepted; key printing differences do not alter the configuration.
- A favourite channel count above the active profile's capacity: reject.

## Research Notes

- https://github.com/dannybloe/harmony-explorations/blob/main/reference/usb-protocol/file-protocol.md — defines the packet layout, file commands, read semantics, write sequence and the `/cfg/usercfg` path used by the 300.
- https://github.com/dannybloe/harmony-explorations/blob/main/reference/architectures/harmony-300-350/README.md — documents architecture 16, the shared 300/350 platform, the four-group allocation and the event `0xFF` binding required for groups above 3.
- https://github.com/dannybloe/harmony-explorations/blob/main/reference/remotes/harmony-300/features.md — states the 300's service limits (four devices, four to five favourites, one Watch TV shortcut) and the absence of a screen.
- https://github.com/dannybloe/harmony-explorations/blob/main/reference/logitech/device-catalogue.md — establishes that Logitech's 2.2 GB catalogue is not redistributable, so IR data must come from open databases.
- https://github.com/av1452/chorus — provides reference IR encoders for NEC, RC5, Sony SIRC, Panasonic/Kaseikyo, Samsung32 and JVC, and confirms the 300 is file-protocol based.
- https://github.com/jaymzh/concordance — provides the established libconcord write discipline and GPLv3 compatibility constraints.

## Assumptions

- The target is Linux x86_64 with libusb; a udev rule is documented so the CLI runs without root.
- v1 targets Harmony 300 firmware 1.4, architecture 16, skins 78 and 79.
- v1 supports short-press device keys only; long press is a later model-profile capability.
- v1 exposes four devices, matching Logitech's service limit; the profile table permits eight in a later version.
- IR protocol coverage starts with the protocols present in Pronto, LIRC and Flipper-IRDB input; unsupported protocols fail explicitly rather than being approximated.
- The project is GPLv3 to remain compatible with concordance, harmony-decompiler and chorus.
- The project is named Remote Rebirth; “Logitech Harmony 300” is used descriptively only, and no Logitech mark appears in the product name.
- “Flash” in this feature means writing configuration only; firmware writes are excluded.
- A GUI is out of scope for this specification and will consume the same core library later.

## Success Criteria

- **SC-001**: On a physical Harmony 300, `detect` reports the correct model, skin, architecture 16 and firmware version.
- **SC-002**: A `backup` followed by `restore --commit` reproduces the original configuration byte for byte, and the remote remains operational.
- **SC-003**: A composed configuration with four devices, a Watch TV shortcut and the profile's full favourite capacity is accepted by a physical Harmony 300; each device key selects its declared device, and every configured key sends its expected code to a bench receiver.
- **SC-004**: Every command succeeds on a machine with networking disabled.
- **SC-005**: A write is refused before erase when the model is wrong, no verified backup exists, the current content differs from the backup, or the container is invalid; the remote's content is unchanged in each case.
- **SC-006**: A forced read-back mismatch produces a non-zero exit, names the backup, and leaves the backup intact.
- **SC-007**: A Harmony 350 model profile can be added without modifying the container parse or emit code, demonstrated by a profile-level test.
- **SC-008**: JSON output validates against the published schema, and every documented exit code is exercised by an automated test.
