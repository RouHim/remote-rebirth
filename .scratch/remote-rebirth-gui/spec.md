# Remote Rebirth — Desktop GUI

Status: ready-for-agent
Source: `.spec/remote-rebirth-gui.md`
Related: `.scratch/remote-rebirth/spec.md` — Harmony 300 offline CLI and core
Domain: `GLOSSARY.md`, `docs/adr/0001`–`0009`

## Problem Statement

The CLI can back up, compose, write, restore and inspect a Harmony 300, but it requires hand-written Model JSON, file paths and selector strings. A Harmony 300 owner who used Logitech's original desktop software expects to add a device, pick commands, click keys on a picture of the remote, and press Update Remote. That software cannot be used: its catalogue is proprietary, its service is disappearing, and its network dependency is the reason this project exists. The CLI is also a poor recovery surface for an owner who does not know canonical key names or Flipper selectors.

## Solution

A native Linux desktop GUI, built with Iced 0.14, that links the same core library as the CLI. It is a full owner application: detect the Remote, manage a Backup library, search a local IR Library, import a Device's full command set, edit Key Maps on a schematic Harmony 300 with a command palette and drag-and-drop, configure volume and channel Routing, edit the Watch TV Activity and Favourites, then Prepare and explicitly Commit a Write or Restore with visible progress and Read-Back.

The GUI is a direct-form editor with no setup wizard. It runs on Linux x86_64 and ships as a plain binary for the MVP. The CLI remains the scripting and recovery surface, with stable JSON and exit codes; both frontends use `remote-rebirth-core`. The core and CLI stay network-free. The GUI is the only component allowed a network client, and only for the optional first-run download of the open Flipper-IRDB library.

## User Stories

### First run and IR Library

1. As an owner, I want a first-run consent screen naming the IR Library source, licence, size and cache path, so that no network request surprises me.
2. As an owner, I want to skip the download and still import local Pronto, LIRC and Flipper files or paste Pronto hex, so that the GUI works offline.
3. As an owner, I want the downloaded archive verified against a pinned SHA-256 before use, so that a corrupted or substituted archive is rejected.
4. As an owner, I want the IR Library cached locally, so that later edits, Compose and Write need no network.
5. As an owner, I want Refresh to be manual, so that the application performs no background update check or telemetry.

### Remote and Backup

6. As an owner, I want the GUI to detect the connected Harmony 300 and show model, Skin, architecture, firmware, GUID and serial, so that I know which Remote I am editing.
7. As an owner, I want the active Model's Skin locked to the connected Remote, so that I cannot compose for the wrong printing.
8. As an owner, I want to create a Backup now and see its digest and validation status, so that I have a recovery point before editing.
9. As an owner, I want a managed Backup list with model, Skin, validation, size, digest and timestamp, so that I can choose a Restore point.
10. As an owner with no Remote, I want to edit and compose offline and see why Write is unavailable, so that I can prepare before plugging in.

### Devices and IR Library

11. As an owner, I want to search the IR Library by brand and model, so that I can find my television without knowing file formats.
12. As an owner, I want Add to Device to import the library Device's full command set, so that I can later bind any command without re-importing.
13. As an owner, I want commands that no key or Activity references marked “not sent”, so that an unused command does not block Compose.
14. As an owner, I want a referenced command with an unsupported protocol to fail with the file, selector, protocol and command, so that I can fix the source.
15. As an owner, I want GUI-only library provenance (source, brand, model) kept outside the portable Model, so that exported Model JSON stays valid for the CLI.

### Key Maps and Routing

16. As an owner, I want a schematic Harmony 300 with clickable keys and a mirrored table, so that I can assign by position or by name.
17. As an owner, I want a searchable command palette and drag-and-drop assignment, so that mapping many keys is fast.
18. As an owner, I want keyboard-only assignment to match mouse assignment, so that the editor is accessible.
19. As an owner, I want occupied Device Keys disabled, so that two Devices cannot share one slot.
20. As an owner, I want inline errors for missing Commands, unknown keys and routing conflicts, so that Compose is never a wall of schema errors.
21. As an owner, I want “Volume buttons: TV” and “Channel buttons: Cable/Sat” settings, so that those keys work regardless of the selected Device.
22. As an owner, I want the target Device's own Key Map bindings to define the routed commands, so that I edit the command once and every Device inherits it.
23. As an owner, I want routed keys shown as inherited on non-target Devices, so that I do not accidentally create a conflicting binding.
24. As an owner, I want a missing target binding to name the Device and key, so that I know what to fix.

### Write and Restore

25. As an owner, I want Prepare to validate, compose, detect, verify the Backup and current Configuration, run the Dry Run and show a summary, so that I see exactly what will be written.
26. As an owner, I want Commit to be a separate explicit action, so that the erase is never a single accidental click.
27. As an owner, I want progress through reset, open, chunks, checksum, flush, close, Read-Back and reboot, so that I know where a failure occurred.
28. As an owner, I want Cancel disabled once the commit starts, so that the Remote is not left half-written.
29. As an owner, I want a Read-Back mismatch to name the Backup and offer Restore, so that recovery is one action away.
30. As an owner, I want Restore to use the same Prepare/Commit flow and to refuse a mismatched or invalid Backup, so that a damaged capture is never replayed.

### Offline and CLI interop

31. As an owner, I want the active Model autosaved and atomically written, so that a crash does not corrupt it.
32. As an owner, I want Export Model and Import Model, so that I can move a configuration between the GUI and the CLI.
33. As an owner, I want a machine with networking disabled to edit, compose, inspect and write with a cached IR Library, so that the tool does not depend on any service.
34. As a script author, I want the CLI's existing JSON and exit codes unchanged, so that automation keeps working.

## Implementation Decisions

### Workspace and core boundary

- Convert the repository to a Cargo workspace:
  - `remote-rebirth-core`: typed operations, Model validation, codecs, transport traits, progress events, no stdout/stderr, no network client.
  - `remote-rebirth-cli`: the existing `remote-rebirth` binary; a thin adapter that initialises the tracing subscriber, builds the runtime and calls `run`. The clap definitions, JSON rendering and exit-code mapping live in the core behind its default `cli` feature.
  - `remote-rebirth-gui`: Iced application, IR Library downloader/indexer, GUI project state.
- The binary name `remote-rebirth` and its documented contract stay unchanged. The GUI binary is `remote-rebirth-gui` for the MVP.
- Core operations return typed reports and accept a progress sink. The GUI runs them on a worker thread and receives events through an Iced subscription. Cancellation is checked between steps and is disabled after reset/open.
- The core keeps the existing `run(args, runtime)` seam for CLI tests; GUI-facing operations are added beside it, not by parsing CLI JSON.

### Model v2

- Add optional `routing` to the Model:
  ```json
  "routing": { "volume": "TV", "channel": "Cable/Sat" }
  ```
- `routing.volume` requires the target Device's `keys` to bind `volume_up`, `volume_down` and `volume_mute`.
- `routing.channel` requires `channel_up` and `channel_down`.
- A non-target Device binding a routed key is rejected. The target's own bindings are the source and stay editable; other Devices show the keys as inherited.
- Compose copies the target's key-map entries into every other Device's map, calling the target Device's send list. The Container already supports this cross-device send; no new section is needed.
- Model schema v2 is optional-routing; v1 Models remain readable. A v2 Model with routing is not readable by a v1 binary.

### Compose and unreferenced Commands

- The v1 rule “every declared Command must be referenced” is removed.
- The referenced set is built from all Device `keys`, the Activity's `power` and `input`, and the routed target's required bindings.
- Compose validates and imports only referenced Commands; unreferenced Commands are omitted from the Container and resolved-model sidecar and remain in the Model.
- An unreferenced Command with an unsupported protocol does not block Compose; binding it produces the normal unsupported-protocol error.
- The 256 KiB limit still applies. The CLI may warn and report unused counts; it does not reject the Model.
- `compose --print-schema` prints v2; the v1 schema remains for historical validation.

### GUI information architecture

- Sidebar: Remote, Devices, Key Map, Activities, Favourites, IR Library, Settings.
- Persistent footer: connection status, Remote identity, save state, estimated Container size.
- Remote page: identity, Backup now, Prepare Write, Restore, Journal/recovery.
- Devices: four Device Key slots (TV, Cable/Sat, DVD, VCR/Aux), add/edit/remove, library import, command list with provenance and “not sent” badges.
- Key Map: Device selector, volume/channel Routing controls, schematic diagram, mirrored table, command palette, drag-and-drop, keyboard assignment.
- Activities: one Watch TV Activity, channel Device, ordered steps with power, input and delay.
- Favourites: positional keys, channel numbers, capacity from the active Skin.
- IR Library: source/licence/version, search, brand/model/command browsing, Add to Device, local import, Pronto paste, Refresh.
- Settings: data paths, Backup management, IR Library refresh, About/licences.
- No wizard. Prepare/Commit is a two-step action, not a wizard.

### IR Library download

- First run is opt-in and names Flipper-IRDB, CC0-1.0, approximate size, source URL and cache path.
- Pin one Flipper-IRDB commit per application release; download the archive over HTTPS with rustls; verify a SHA-256 pinned in the binary; extract under `$XDG_DATA_HOME/remote-rebirth/ir-library/`.
- Skip or failure leaves local file import and Pronto paste fully usable. Refresh re-downloads the pinned archive only on explicit action. No telemetry, analytics, automatic update check or background refresh.
- The downloader lives only in the GUI crate. The core and CLI contain no network client.
- Flipper-IRDB is data, not linked code; display the source and licence.

### Persistence

- App data: `$XDG_DATA_HOME/remote-rebirth/`, falling back to `$HOME/.local/share/remote-rebirth/`.
- `model.json`: active Model v2, portable and CLI-compatible.
- `gui.json`: GUI-only state (window size, selections, library provenance, recents).
- `backups/`: managed Backup Containers and sidecars; keep all, manual delete.
- `ir-library/`: pinned archive, extracted index, version marker.
- `settings.json`: preferences, including download consent.
- Autosave atomically; Export writes a standalone Model JSON; Import validates before replacing the active Model.
- Journal remains at `$XDG_STATE_HOME/remote-rebirth/journal.jsonl`, shared with the CLI.

### Safety

- The GUI never bypasses a core gate: verified Backup, model/Skin match, current Configuration equal to the Backup or empty, size and checksum, checksum check `m`, Read-Back equality, reboot policy.
- Automatic Backup before every committed Write; the Backup list keeps all entries.
- Prepare validates, composes, detects, verifies, dry-runs and shows a summary. Commit is separate and explicit.
- Cancel is disabled after reset/open. A failure surfaces the Journal path, Backup name and Restore action.

### Visual and interaction design

- Owner's-manual world: uncoated paper (#F6F0E3), a cloth-bound cover rail (#8C3F25), one oxide spot (#B34824), and grass (#5B7153) for confirmed state. Serif section headings, sans work text, mono only for codes, counts and file paths. No cards: pages, hairline rules and marginal notes.
- Signature: the Harmony 300 is drawn once as a numbered key diagram; its callouts match the numbered work table. The Remote page carries a stepped fore-edge phase rail (Detect, Edit, Prepare, Commit, Read-Back).
- Flat paper depth: one 1px rule weight, a 2px sheet edge, a single ambient sheet shadow, and an 18px radius only on the schematic remote body. Controls are square, flat and stateful.
- Fixed header, sheet footer and machine status bar; only the page body scrolls. The sheet footer carries the section page number and save state.
- The configured Model opens on Buttons; a Model without Devices opens on Remote so Detect and Backup come first. No wizard.
- Bundled OFL faces: Public Sans (work text), Source Serif 4 SemiBold (headings), Spline Sans Mono (codes and counts). No system display face.
- English-only v1; user strings isolated for later translation.
- Keyboard: shortcuts for chapter navigation, backup/review/detect, device and key traversal, command assignment and dismissal; visible focus on buttons and full Tab traversal remain open because Iced 0.14 exposes no button focus state. No colour-only state; AA text contrast against paper and cloth.

### Delivery slice

- First shippable GUI: core prerequisites (workspace, Model v2 routing, unreferenced-Command filtering, typed operations, progress events) plus Detect, Backup now, one Device from a local file or inline hex, Key Map with diagram/table/palette and Routing, Compose, Prepare, Commit, Read-Back and Restore.
- IR Library downloader/browser, Favourites polish and Activity polish follow. Local file/hex import is available in the first slice.

## Testing Decisions

- Core tests: Model v2 parse, v1 compatibility, routing expansion, routed-key conflicts, missing target bindings, unreferenced-command filtering, resolved-sidecar content, 256 KiB limit.
- CLI regression: existing commands, JSON and exit codes unchanged; a Model with unreferenced Commands composes with a warning.
- GUI tests: reducer/update tests for Key Map, Routing, validation and Prepare/Commit; Iced simulator tests for critical interactions where practical.
- Headless visual verification: `sway-headless` / `wayland-1`, `grim` screenshots at minimum and default sizes; the `design-taste-iced` pre-flight matrix.
- Hardware: physical Harmony 300 detect, backup, library import, key mapping, routing, compose, dry run, commit, Read-Back, reboot, restore.
- Bench IR receiver: a non-TV Device's volume keys send the TV's codes; a non-cable Device's channel keys send the cable box's codes; the target's own keys still send its own codes.
- No hardware in CI; fake Remote and fake clock keep the suite deterministic.

## Out of Scope

- Firmware reads or writes.
- IR learning or capture.
- Logitech's catalogue or proprietary IR data.
- Automatic IR Library updates, telemetry or analytics.
- Windows, macOS or non-x86_64 Linux.
- AppImage, Flatpak, AUR or installer automation for the MVP.
- Localization.
- Long press.
- Harmony 350 Model Profile enablement; recognised and refused.
- More than one Watch TV Activity.
- Routing digits, Enter, previous channel or Input; volume and channel up/down only.
- Favourite channels with leading zeros.
- Root or udev installation automation.
- Automatic retry after a failed Write.
- Preserving the Remote's log area.

## Further Notes

- Cross-device key maps are already supported by the Container. The reference documents a Device's map borrowing another Device's send list, including the DVD map sending volume to the television. Routing adds the Model-level statement and Compose expansion; no Container section is added.
- Channel routing uses the same mechanism but is not directly measured in the reference; the bench receiver is the acceptance gate.
- The GUI's first-run download is a deliberate, narrow exception to the CLI's offline posture. No request runs without an explicit owner action.
- GUI tracker: this spec. No GUI tickets have been created yet.

## Implementation Status

The first delivery slice is implemented:

- Workspace: `remote-rebirth-core`, `remote-rebirth-cli`, `remote-rebirth-gui`; the CLI keeps its `remote-rebirth` binary and contract.
- Model schema v2 with optional volume/channel Routing; v1 Models remain readable.
- Compose filters unreferenced Commands and records only composed Commands in the resolved sidecar.
- Typed `gui_api` operations with progress events; cooperative Cancel before Commit; Commit Cancel disabled.
- Iced GUI rebuilt as an owner's manual: Remote, Devices, Key Map (numbered diagram, table, command index and Routing), Activities, Favourites, IR Library (local folder), Settings; Detect on open, managed Backup list with stamped validation states, standalone Compose, Prepare/Commit Write and Restore, and a separate `gui.json` for GUI-only state.
- Keyboard shortcuts cover chapter navigation (`Ctrl+1…7`), backup/review/detect, device and key traversal, first-match command assignment (`Ctrl+Enter` or Enter in the index), and dismissal. Full focus traversal and a button focus ring remain open.
- Local IR Library browser: scans a chosen folder of Flipper `.ir` files, searches brand/model/command, and imports a whole command set into the selected Device. It makes no network request; the pinned Flipper-IRDB downloader remains next.
- Tests: 136 core tests and 9 GUI tests; `cargo clippy --workspace --all-targets` clean; the redesigned GUI captured under `sway-headless` with `grim` at 1240×820 and the 1040×680 minimum in `.impeccable/review/`.
- Hardware read path on the Harmony 300: Detect, Backup (24,725 B, validation `ok`), Compose and a Write Dry Run (commit `false`, 1 chunk, 37 data packets) verified on 2026-11-06. No committed write or Read-Back was performed; that remains the release gate.

The next slice is the pinned Flipper-IRDB downloader with first-run consent, drag-and-drop assignment, field-level inline validation, Backup retention/filtering polish, localization, and packaging. These are specified above and are not claimed as implemented.
