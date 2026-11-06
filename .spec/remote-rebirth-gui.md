# Feature Specification: Remote Rebirth — Desktop GUI

**Created**: 2026-11-06
**Status**: Approved
**Input**: A native Linux desktop application that gives a Harmony 300 owner the original Logitech software's editing comfort without its catalogue, account or service. The CLI remains supported.
**Related**: `.spec/remote-rebirth.md` — Harmony 300 offline CLI and core.

## Goal

A Harmony 300 owner cannot reconfigure the Remote once Logitech's configuration service disappears. The CLI already detects, backs up, composes, writes, restores and inspects a Configuration, but it requires hand-written JSON and file selectors. This feature adds a native desktop GUI that edits the same Model, resolves IR Sources from a searchable local IR Library, maps keys on a visual Harmony 300, and drives the same verified Write and Restore path.

The GUI is a full owner application, not a thin CLI wrapper: Devices, Key Maps, Watch TV, Favourites, the IR Library, backup management, Write and Restore all live in one window. It is a direct-form editor with no setup wizard. The CLI and the core library remain network-free. The GUI is the only component allowed a network client, and only for the optional first-run IR Library download.

## Relationship to the CLI

The CLI is the scripting and recovery surface. The GUI is the owner-facing surface. Both link the same core library; neither shells out to the other. The CLI's documented JSON output and exit codes remain stable. The Model JSON remains the interchange format: the GUI's active Model is ordinary Model JSON that the CLI can compose, and an exported Model opens in the GUI.

The CLI-only v1 was one Cargo package. This feature converts the repository to a workspace:

- `remote-rebirth-core` — typed operations, validation, codecs, transport traits, progress events, no stdout/stderr, no network client.
- `remote-rebirth-cli` — the existing `remote-rebirth` binary: clap, tracing subscriber, JSON rendering, exit codes.
- `remote-rebirth-gui` — the Iced application, IR Library downloader and indexer, GUI-only project state.

## User Scenarios

### Scenario 1 — First run and IR Library (P1)

An owner installs the GUI and wants a searchable IR Library without hand-selecting files.

**Acceptance**

1. Given a first run, when the GUI starts, then it shows a consent screen naming the source (`Flipper-IRDB`), licence (`CC0-1.0`), approximate download size, cache path and the fact that it is a one-time network request.
2. Given the owner accepts, when the download completes, then the archive is verified against a SHA-256 pinned in the application, extracted under the app data directory, indexed, and the GUI opens with the IR Library available.
3. Given the owner skips, or the download fails, when the GUI opens, then local Pronto/LIRC/Flipper file import, inline Pronto hex and paste still work; the IR Library page offers a retry.
4. Given a cached IR Library, when the machine has no network, then every GUI operation except the explicit download/refresh succeeds.
5. Given the owner opens Settings → IR Library, when they press Refresh, then the pinned archive is re-downloaded only on explicit action and verified again; no update check, telemetry or background request runs.

### Scenario 2 — Detect and back up the Remote (P1)

An owner connects the Remote and captures a recovery point before editing.

**Acceptance**

1. Given a connected Harmony 300, when the GUI opens or the owner presses Refresh, then the footer reports model, Skin, architecture, firmware version, GUID and serial number.
2. Given a connected Remote and a chosen backup target, when the owner presses Backup now, then the GUI writes a Backup Container and sidecar, shows the SHA-256 and validation status, and adds it to the managed Backup list.
3. Given no Remote, when the GUI opens, then it remains usable for offline editing and names the missing device in the Remote page.
4. Given a recognised but unsupported model, when the GUI opens, then it reports the model and refuses Detect-dependent Write actions without opening the Remote for writing.

### Scenario 3 — Build a Device from the IR Library (P1)

An owner adds a television and wants its full command set without typing command names or file selectors.

**Acceptance**

1. Given a downloaded IR Library, when the owner searches a brand and model and chooses Add to Device, then the GUI creates a Device with the library's full command set, records the library source and brand/model in GUI project state, and shows the commands with their provenance.
2. Given an imported Device, when the owner edits its Key Map, then commands that no key or Activity references are marked “not sent” rather than blocking the project.
3. Given a command with a protocol the core cannot encode, when it is unreferenced, then Compose and Write remain available; when the owner binds it, then the GUI refuses the binding and names the protocol and command.
4. Given a local Pronto, LIRC or Flipper file, when the owner imports it, then the same Device and Command representation is used as for a library import; inline Pronto hex remains available.

### Scenario 4 — Edit Key Maps comfortably (P1)

An owner assigns commands to the Remote's physical keys.

**Acceptance**

1. Given a selected Device, when the owner opens Key Map, then the GUI shows a schematic Harmony 300 diagram and a mirrored table of canonical keys, both reflecting the same bindings.
2. Given a key selected on the diagram, when the owner chooses a command from the searchable command palette, then the binding is written to the Model and both views update.
3. Given a command dragged from the palette onto a key, when dropped, then the same binding is created; keyboard-only assignment produces the same result.
4. Given an occupied Device Key, when the owner adds another Device, then that slot is disabled and the assignment cannot be duplicated.
5. Given a Key Map with a missing Command, an unknown key or a routed-key conflict, then the field is marked inline and Compose/Write is disabled until resolved.

### Scenario 5 — Route volume and channel (P1)

An owner wants volume to control the television and channel to control the cable box regardless of the selected Device.

**Acceptance**

1. Given the Key Map page, when the owner sets “Volume controls: TV” and “Channel controls: Cable/Sat”, then the Model records `routing.volume = "TV"` and `routing.channel = "Cable/Sat"`.
2. Given a routing target, when its own Key Map binds `volume_up`, `volume_down` and `volume_mute` (or `channel_up` and `channel_down`), then Compose copies those bindings into every other Device's Key Map.
3. Given the target does not bind a required key, then the GUI names the missing key and the target Device and blocks Compose/Write.
4. Given a non-target Device already binds a routed key, then the GUI rejects the conflict and shows the target that owns the key.
5. Given routing is enabled, when the owner selects another Device, then the routed keys are shown as inherited and are not individually editable there; the target's own bindings remain editable.

### Scenario 6 — Prepare and Write (P1)

An owner commits a composed Configuration without risking the only working one.

**Acceptance**

1. Given a valid Model, when the owner presses Prepare, then the GUI validates, composes, detects the Remote, verifies the Backup and current Configuration, runs the Dry Run and shows a summary: Remote identity, Backup digest, Container size, chunks, data packets and reboot choice.
2. Given the summary, when the owner presses Write to remote, then the committed Write runs with visible progress through reset, open, chunked write, checksum check, flush, close, Read-Back and reboot.
3. Given no verified Backup, a Skin mismatch, a current Configuration that differs from the Backup, or an invalid Container, then Prepare refuses before any erase and names the failed check.
4. Given a committed Write, then a read-back mismatch exits the operation with the recovery message, names the Backup and offers Restore.
5. Given a commit has started, then Cancel is disabled; before the commit starts, Cancel aborts without opening the Remote for writing.

### Scenario 7 — Restore (P1)

An owner returns the Remote to a known-good Backup.

**Acceptance**

1. Given a managed Backup whose sidecar validation is `ok`, when the owner selects Restore, then the GUI runs the same Prepare summary and requires the separate Restore confirmation.
2. Given a Backup whose model or Skin differs from the connected Remote, or whose validation is not `ok`, then Restore refuses before opening the Remote for writing.
3. Given a committed Restore, then the Read-Back equals the Backup byte for byte and the Remote remains operational after reboot.

### Scenario 8 — Offline editing and export (P2)

An owner prepares a Configuration before plugging in the Remote, or moves it to the CLI.

**Acceptance**

1. Given no Remote, when the owner edits Devices, Key Maps, the Activity and Favourites, then the GUI autosaves the active Model and validates it against the selected Skin.
2. Given an exported Model, when the owner runs `remote-rebirth compose --model <path>`, then the CLI accepts it; Model schema v2 routing is honoured and v1 Models remain readable.
3. Given a machine with networking disabled, when the owner edits, composes, inspects or writes with a cached IR Library, then every operation succeeds.

## Functional Requirements

- **GUI-FR-001**: The GUI MUST be a native Linux x86_64 application built with Iced 0.14, shipped for the MVP as a plain binary; no AppImage, Flatpak or AUR artifact is required for the first release.
- **GUI-FR-002**: The repository MUST be a Cargo workspace with `remote-rebirth-core`, `remote-rebirth-cli` and `remote-rebirth-gui` packages; the CLI binary name and contract MUST remain `remote-rebirth`.
- **GUI-FR-003**: The core library MUST expose typed operations and progress events, MUST NOT write to stdout or stderr, and MUST NOT contain a network client. The CLI adapter owns clap, JSON output, tracing and exit codes.
- **GUI-FR-004**: The GUI MUST use a sidebar with Remote, Devices, Key Map, Activities, Favourites, IR Library and Settings, plus a persistent footer with connection status, Remote identity, save state and estimated Container size.
- **GUI-FR-005**: The GUI MUST be a direct-form editor: no setup wizard, no mandatory next/back sequence. A first-run consent modal for the IR Library download is permitted.
- **GUI-FR-006**: The GUI MUST detect and display the Remote identity, Skin, architecture, firmware version, GUID and serial number; it MUST lock the active Model's Skin to a connected Remote and MUST require an explicit resolution when the Model's Skin differs.
- **GUI-FR-007**: The GUI MUST manage an active Model under `$XDG_DATA_HOME/remote-rebirth/` (falling back to `$HOME/.local/share/remote-rebirth/`), autosave edits atomically, and keep GUI-only state (window state, library provenance, selections) separate from the portable Model JSON.
- **GUI-FR-008**: The GUI MUST provide Export Model and Import Model actions. Exported Models MUST validate against the Model schema; imported v1 Models MUST be accepted and may be upgraded in memory.
- **GUI-FR-009**: The GUI MUST manage a Backup library: automatic Backup before every committed Write, keep all Backups, manual delete, and display model, Skin, validation status, size, digest and timestamp.
- **GUI-FR-010**: The GUI's first-run IR Library download MUST be opt-in, pin one Flipper-IRDB commit per application release, verify a SHA-256 before extraction, cache under the app data directory, and work fully with local files if skipped or failed.
- **GUI-FR-011**: The IR Library downloader MUST be the only network client in the product. It MUST NOT contain telemetry, analytics, automatic update checks or background refreshes. The CLI and core MUST remain network-free.
- **GUI-FR-012**: The IR Library MUST be searchable by brand, model and command and MUST support importing a Device's full command set. GUI-only provenance (library source, brand, model) MUST be stored outside the portable Model.
- **GUI-FR-013**: The Key Map editor MUST provide a schematic Harmony 300 diagram and a mirrored table, click-to-select, a searchable command palette, drag-and-drop assignment, keyboard navigation and clear unassigned/conflict states.
- **GUI-FR-014**: The GUI MUST expose volume and channel Routing. `routing.volume` names a Device whose own `volume_up`, `volume_down` and `volume_mute` bindings are copied into every other Device's Key Map; `routing.channel` does the same for `channel_up` and `channel_down`.
- **GUI-FR-015**: A non-target Device MUST NOT bind a routed key. The target Device MUST bind every key required by the routing function. Conflicts and missing target bindings MUST be rejected before Compose.
- **GUI-FR-016**: Compose MUST accept Model schema v2, MUST continue to accept v1, MUST validate and import only Commands referenced by a Key Map, the Watch TV Activity or a routed target's required bindings, and MUST omit unreferenced Commands from the Container and resolved-model sidecar.
- **GUI-FR-017**: An unreferenced Command MUST be allowed in the Model even if its IR Source cannot be encoded; binding it MUST fail with the file, selector, protocol and Command. A referenced Command with an unsupported protocol MUST still fail.
- **GUI-FR-018**: The GUI MUST implement Prepare then Commit for Write and Restore. Prepare validates, composes, detects, verifies the Backup and current Configuration, runs the Dry Run and shows a summary; Commit is a separate explicit action.
- **GUI-FR-019**: The GUI MUST NOT bypass any core safety gate: verified Backup, matching model and Skin, current Configuration equal to the Backup or empty, Container size and checksum, checksum check `m`, Read-Back equality, and reboot policy.
- **GUI-FR-020**: During a committed Write or Restore the GUI MUST show step-level progress and MUST disable Cancel once the reset or open-for-writing has been sent.
- **GUI-FR-021**: The GUI MUST surface a failed operation's Journal path, the Backup name and a Restore action on Read-Back mismatch or protocol failure.
- **GUI-FR-022**: The GUI MUST use one modern calm light theme, one accent colour, a neutral surface system, monospace for data values, and a schematic vector Harmony 300 diagram. Dark mode, photorealistic renders and localization are out of scope for v1.
- **GUI-FR-023**: The GUI MUST remain usable at its minimum window size without clipped labels or unreachable controls; fixed chrome stays outside scroll regions.
- **GUI-FR-024**: The GUI MUST be English-only in v1, with user strings isolated for later translation.
- **GUI-FR-025**: The GUI MUST show recognised-but-unsupported models (Harmony 350, Harmony 200) and refuse Write/Restore for them; it MUST NOT expose firmware update, IR learning or long-press controls.

## Model v2 and Compose Changes

### Schema

Model schema v2 adds one optional object:

```json
{
  "schema_version": 2,
  "routing": {
    "volume": "TV",
    "channel": "Cable/Sat"
  }
}
```

- `routing` is optional. `volume` and `channel` are optional Device names.
- A v1 Model has no `routing`. A v2 Model may omit it; behaviour is then identical to v1.
- A v2 Model with `routing.volume = "TV"` requires `TV.keys` to bind `volume_up`, `volume_down` and `volume_mute`.
- A v2 Model with `routing.channel = "Cable/Sat"` requires `Cable/Sat.keys` to bind `channel_up` and `channel_down`.
- Every non-target Device MUST NOT bind a key owned by an enabled routing function. This is a validation error, not a silent override.
- The target Device's own Key Map is the source of truth for the routed bindings; it stays editable. Other Devices show the routed keys as inherited.
- Compose copies the target's key-map entries into every other Device's map. The copied entry calls the target Device's send list, producing the cross-Device send the architecture already supports.
- If the target Device is missing a required binding, Compose refuses and names the Device and key. If a non-target Device binds a routed key, Compose refuses and names both Devices.

### Unreferenced Commands

The v1 rule “every declared Command must be referenced” is replaced:

- The Model may declare Commands that no key or Activity references. This is normal after importing a library Device's full command set.
- Compose builds the referenced set from all Device `keys`, the Activity's `power` and `input` Commands, and the routed target's required bindings.
- Only referenced Commands are validated for encodability, imported and laid out as IR records and send lists.
- Unreferenced Commands are omitted from the Container and the resolved-model sidecar. They remain in the Model for editing.
- An unreferenced Command with an unsupported protocol does not block Compose. Binding it produces the normal unsupported-protocol error.
- The CLI may warn on stderr and count unused Commands in JSON output; it MUST NOT reject the Model for them.
- The 256 KiB limit still applies to the composed Container.

### Compatibility

- The CLI accepts Model schema v1 and v2. `compose --print-schema` prints the v2 schema; the v1 schema remains available for validating historical Models.
- A v2 Model with routing is not readable by a v1 binary; the CLI and GUI ship together.
- Inspect output is unchanged: routed key-map entries appear as ordinary entries pointing at the target Device's send lists. The resolved-model sidecar gains the routing object and records only composed Commands.

## Architecture

- `remote-rebirth-core`:
  - Public typed operations for detect, backup, inspect, compose, prepare-write, commit-write, prepare-restore, commit-restore.
  - `Progress` events for step transitions, bytes written and read-back comparison; a callback or channel sink supplied by the caller.
  - A cancellation token checked between steps; the commit path itself is not interruptible after the reset/open.
  - Pure Model validation and routing expansion; no I/O beyond files, transport traits and clock.
  - No `clap`, no `tracing-subscriber`, no stdout/stderr, no HTTP client.
- `remote-rebirth-cli`:
  - The existing `run(args, runtime) -> Exit` seam, clap definitions, JSON rendering, stderr progress and exit-code mapping.
  - Uses the core operations; behaviour changes only where this spec requires (v2 Models, unreferenced Commands, routing).
- `remote-rebirth-gui`:
  - Iced 0.14 application. Core operations run on a worker thread; progress events arrive through an Iced subscription. The UI never blocks on USB or file I/O.
  - A single USB session owner; concurrent CLI/GUI operations are refused by the transport or surface a device-busy error.
  - IR Library downloader and indexer are GUI-only. The archive is fetched over HTTPS with rustls, verified against a pinned SHA-256, extracted and indexed locally.
  - No async runtime is required in the core; the GUI may use threads and channels.
- Workspace migration:
  - The existing flat modules move into `remote-rebirth-core` with their tests; the CLI package keeps only the adapter and CLI-facing tests.
  - The GUI package depends on the core, Iced, serde and the downloader. The CLI does not depend on Iced or the downloader.

## UX and Visual Design

- **Navigation**: sidebar with Remote, Devices, Key Map, Activities, Favourites, IR Library, Settings. Fixed footer: connection status, Remote identity, save state, estimated Container size.
- **Remote page**: identity card, Backup now, Prepare Write, Restore, Journal/recovery.
- **Devices page**: four Device Key slots with printed labels (TV, Cable/Sat, DVD, VCR/Aux); add/edit/remove; library import; command list with provenance and “not sent” badges.
- **Key Map page**: Device selector; routing controls for volume and channel; schematic Harmony 300 diagram; mirrored table; command palette; drag-and-drop; keyboard assignment; inherited/locked routed keys.
- **Activities page**: one Watch TV Activity; channel Device; ordered steps with power Command, input Command and delay.
- **Favourites page**: positional keys with channel numbers, capacity from the active Skin, inline validation.
- **IR Library page**: source/licence/version, search, brand/model/command browsing, Add to Device, local file/folder import, inline Pronto paste, refresh.
- **Settings page**: data paths, Backup list management, IR Library download/refresh, About/licences.
- **Visual system**: modern calm light theme; neutral surfaces; one blue accent; monospace for scan codes, digests, paths and counts; schematic vector remote diagram; one radius scale; explicit spacing scale; full hover/pressed/disabled/focus states.
- **No wizard**: forms and modals only. Prepare/Commit is a two-step action, not a wizard.
- **Accessibility**: keyboard reachability for every action, visible focus, labelled controls, no colour-only state, minimum window size tested.

## Persistence

- App data directory: `$XDG_DATA_HOME/remote-rebirth/` with `$HOME/.local/share/remote-rebirth/` fallback.
- `model.json`: the active Model v2, the portable interchange artifact.
- `gui.json`: GUI-only state (window size, last selections, library provenance, recents).
- `backups/`: managed Backup Containers and sidecars; keep all, manual delete.
- `ir-library/`: pinned Flipper-IRDB archive, extracted index and version marker.
- `settings.json`: user preferences, including whether the IR Library download was accepted.
- Autosave writes atomically (temporary file plus rename). Export writes a standalone Model JSON; Import validates before replacing the active Model.
- The Journal remains at `$XDG_STATE_HOME/remote-rebirth/journal.jsonl`, shared with the CLI.

## Delivery Slice

The first shippable GUI is the end-to-end thin slice, not the full editor:

- Core prerequisites: workspace split, Model v2 routing, unreferenced-Command filtering, typed operations and progress events.
- GUI slice: Detect, Backup now, one Device created from a local Pronto/LIRC/Flipper file or inline hex, Key Map with the schematic diagram, table, command palette and volume/channel Routing, Compose, Prepare, Commit, Read-Back and Restore.
- The IR Library downloader and browser, Favourites polish and Activity polish follow in the next slice. Local file/hex import is available in the first slice, so no library is required to prove the path.

## Testing and Acceptance

- Core tests cover Model v2 parsing, v1 compatibility, routing expansion, routed-key conflicts, missing target bindings, unreferenced-command filtering, resolved-sidecar content and Container size.
- CLI regression tests prove the existing commands, JSON and exit codes remain stable, and that a Model with unreferenced Commands composes with a warning.
- GUI tests cover the reducer/update logic for the Key Map, routing, validation and Prepare/Commit state; Iced simulator tests cover the critical interactions where practical.
- Headless verification uses the existing `sway-headless`/`wayland-1` compositor and `grim` screenshots at the minimum and default window sizes; the design pre-flight matrix is applied to real screenshots.
- Hardware acceptance on a physical Harmony 300: detect, backup, library import, key mapping, routing, compose, dry run, commit, Read-Back, reboot, restore.
- Bench IR receiver acceptance: a non-TV Device's volume keys send the TV's volume codes; a non-cable Device's channel keys send the cable box's channel codes; a target Device's own keys still send its own codes.
- No hardware in CI; the fake Remote and fake clock keep the suite deterministic.

## Out of Scope

- Firmware reads or writes.
- IR learning or capture from another remote.
- Logitech's device catalogue or any proprietary IR data.
- Automatic IR Library updates, background refresh, telemetry or analytics.
- Windows, macOS or non-x86_64 Linux.
- AppImage, Flatpak, AUR or installer automation for the MVP; the GUI ships as a plain binary.
- Localization.
- Long press.
- Enabling the Harmony 350 Model Profile; it is recognised and refused.
- More than one Watch TV Activity.
- Routing digits, Enter, previous channel or Input; v1 routes volume and channel up/down only.
- Favourite channels with leading zeros.
- Root or udev installation automation; the rule remains documented.
- Automatic retry after a failed Write.
- Preserving the Remote's log area.

## Further Notes

- Cross-Device key maps are already supported by the Container. `harmony-explorations` documents a Device's map borrowing another Device's send list, including the DVD map sending volume to the television. Routing adds the Model-level statement and Compose expansion; it does not add a new Container section.
- Channel routing uses the same cross-Device mechanism but is not directly measured in the reference. The bench receiver check is the acceptance gate.
- Flipper-IRDB is CC0-1.0; the downloaded archive is data, not linked code. The application displays the source and licence. No Logitech data is downloaded, bundled or embedded.
- The GUI's first-run download is a deliberate, narrow exception to the CLI's offline posture. The core and CLI still contain no network client, and the GUI makes no request without an explicit owner action.
- The GUI is Linux-first. The core stays portable, but Windows/macOS require a transport and sysfs/udev replacement before they can be claimed.
- The design pass uses the `design-taste-iced` pre-flight matrix; visual verification is performed on the headless compositor with `grim`.
