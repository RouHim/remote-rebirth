# Remote Rebirth — Harmony 300 Offline CLI

Status: ready-for-agent
Source: `.spec/remote-rebirth.md`
Domain: `GLOSSARY.md`, `docs/adr/0001`–`0007`

## Problem Statement

A Logitech Harmony 300 owner cannot reconfigure the Remote once Logitech's configuration service disappears. The vendor client depends on a Logitech account, a network connection, and a device catalogue that cannot be redistributed. Existing offline tools either target other Harmony generations or do not provide a verified Backup and Restore path, so an owner who experiments risks erasing the only working Configuration with no recovery point.

## Solution

A native Rust CLI for Linux x86_64 that operates entirely offline. It identifies the Remote over USB, reads its identity and Configuration, writes a Backup with a metadata sidecar, Composes a new Container from a declarative JSON model plus locally imported IR Sources, and Writes or Restores the Container with a checksum check, a Read-Back, and a Journal.

v1 targets Architecture 16, the Harmony 300 Model Profile, skins 78 and 79, firmware 1.4, four Devices, four IR groups, one Watch TV Activity, and four or five Favourites. The CLI is the v1 surface; a GUI is out of scope, and the core library is structured so a later GUI can consume it.

## User Stories

### Detection and identity

1. As an owner, I want `detect` to identify the connected Remote, so that I know the CLI can see it.
2. As an owner, I want `detect` to report model, Skin, architecture, firmware version, hardware version, GUID, and serial number, so that I can confirm compatibility before any Write.
3. As an owner, I want `detect` to report a recognised but unsupported model without writing, so that I do not risk a wrong device.
4. As an owner, I want `detect` to resolve the model from the USB device descriptor and `/sys/sysinfo`, not from the shared product ID alone, so that the reported identity is trustworthy.
5. As an owner with no Remote connected, I want a non-zero exit and a message naming the missing device, so that I know to connect it.
6. As an owner with more than one matching Remote connected, I want an error listing the candidates, so that I can disconnect all but one rather than have the CLI guess.
7. As a script author, I want `detect --json` to emit a versioned object, so that I can automate identity checks.

### Backup

8. As an owner, I want `backup --out <path>` to write the Container and a JSON sidecar, so that I have a recovery point before any Write.
9. As an owner, I want the sidecar to record model, Skin, architecture, firmware version, GUID, serial number, byte size, SHA-256 digest, UTC timestamp, and validation status, so that I can verify the Backup later.
10. As an owner, I want `backup` to refuse to overwrite an existing file unless I pass `--overwrite`, so that I do not lose an earlier Backup.
11. As an owner, I want `backup` to reconstruct the `PTYY` trailer omitted by `/cfg/usercfg`, so that the Backup file is a valid Container.
12. As an owner, I want `backup` to preserve a readable but invalid Configuration and mark it `checksum_mismatch` or `unreadable`, so that I can keep evidence and attempt recovery.
13. As an owner with no Remote connected, I want `backup` to write no file, so that I do not mistake an empty output for a Backup.
14. As an owner of an Architecture 16 Remote with an unsupported Model Profile, I want `backup` to remain available because it is read-only, so that I can still preserve the device state.

### Inspect

15. As an owner, I want `inspect --file <path> --json` to list Container metadata, Devices, Device Key placement, key maps, the Watch TV Activity, Favourites, and IR Records, so that I can understand what is on the Remote.
16. As a maintainer, I want `inspect` output to validate against a published JSON schema, so that other tools can rely on it.
17. As an owner, I want `inspect` to reject a file that is not an Architecture 16 Container with a specific parse error and no partial JSON on stdout, so that I can diagnose the file.
18. As a maintainer, I want `inspect` to expose unknown raw slots with offset, length, SHA-256, and a `known` flag, so that I can diff and diagnose without the parser guessing.
19. As an owner, I want to inspect two files separately and diff their JSON, so that I can see what changed between Configurations.
20. As a script author, I want `inspect --print-schema`, so that I can retrieve the schema from the binary.

### Compose

21. As an owner, I want to declare Devices, Device Key assignments, Commands, key bindings, the Watch TV Activity, and Favourites in one JSON model, so that I can build a Configuration offline.
22. As an owner, I want `compose --model <model.json> --out <container.bin>` to work without a Remote, so that I can prepare a Configuration before plugging in.
23. As an owner, I want Compose to reject duplicate Device Key assignments, so that each Device Key selects exactly one Device.
24. As an owner, I want Compose to reject a model that exceeds the active Model Profile's Device, IR group, or Favourite limits, so that I do not build an invalid Container.
25. As an owner, I want Compose to reject a Command whose IR Source cannot be encoded, naming the file, selector, protocol, and Command, so that I know what to fix.
26. As an owner, I want IR Sources imported from Pronto hex, LIRC configuration files, and Flipper-IRDB files, so that I can use open IR data.
27. As an owner, I want timing-bearing IR Sources stored as raw pulse records, so that the captured waveform is preserved rather than approximated.
28. As an owner, I want parsed or shorthand IR Sources encoded with the six supported protocol encoders, so that common NEC, RC5, Sony SIRC, Kaseikyo, Samsung32, and JVC commands work.
29. As an owner, I want per-Command provenance recorded in a resolved-model sidecar, so that I can audit which file and selector produced each IR Record.
30. As an owner, I want Compose to bind firmware event `0xFF` to the action list called by the `0x30` entry, so that IR groups above 3 work on the Harmony 300.
31. As an owner, I want Compose to reject a Container larger than 256 KiB, so that the Remote's configuration region is not exceeded.
32. As an owner, I want Compose to use the Skin declared in the model for Favourite capacity, so that skin 78 accepts five Favourites and skin 79 accepts four.
33. As an owner, I want a Favourite channel with a leading zero rejected explicitly, so that I do not get a silently wrong channel.
34. As an owner, I want Compose to refuse to overwrite an existing Container or resolved-model sidecar unless I pass `--overwrite`, so that I do not clobber a Backup.

### Write

35. As an owner, I want `write` to default to a Dry Run, so that I can validate before touching the Remote.
36. As an owner, I want the Dry Run to validate the model, the Backup, the Container size, the Container checksum, and the write plan without opening `/cfg/usercfg` for writing, so that I catch errors before the erase.
37. As an owner, I want `write --commit` to require a verified Backup whose model and Skin match the connected Remote, so that I always have a recovery point.
38. As an owner, I want `write --commit` to verify that the Remote's current Configuration equals the Backup or is empty, so that I do not overwrite an unexpected Configuration.
39. As an owner, I want the committed Write to follow reset, open for writing, chunked write, checksum check, flush only on `m`, close, Read-Back, and reboot, so that the sequence matches the vendor client.
40. As an owner, I want the Read-Back to append the omitted `PTYY` marker and compare byte-for-byte with the written Container, so that I know the Remote holds the intended bytes.
41. As an owner, I want a Read-Back mismatch to skip the reboot, exit non-zero, and name the Backup, so that I can run Restore instead of booting a bad Configuration.
42. As an owner, I want a checksum refusal to prevent the flush, so that a bad Container is never committed.
43. As an owner, I want every failure to Journal the last acknowledged step, so that I can recover after a disconnect.
44. As an owner, I want a malformed packet to close the session and reopen a fresh one before reporting, so that the session is not left poisoned.
45. As an owner, I want `--no-reboot` to suppress the final reboot, so that I can inspect or retry without a power cycle.
46. As an owner, I want the CLI never to open a firmware path for writing, so that firmware stays out of scope and safe.

### Restore

47. As an owner, I want `restore` to default to a Dry Run, so that I can validate a Backup before writing.
48. As an owner, I want `restore --commit` to write the Backup Container and Read-Back byte-for-byte, so that I can return to a known-good Configuration.
49. As an owner, I want Restore to refuse a Backup whose model or Skin differs from the connected Remote before opening for writing, so that I do not write a Backup intended for a different model.
50. As an owner, I want Restore to work after a failed Write, so that the Backup is a real recovery path.

### Profiles and operations

51. As a maintainer, I want Model Profile data to be data-driven, so that adding a Harmony 350 profile does not change the Container parse or emit code.
52. As an owner with a Harmony 350, I want Detect to report it as recognised but unsupported and refuse all Writes, so that I know v1 does not support it.
53. As a maintainer, I want a profile-level test that registers a new Model Profile and exercises Detect, Compose, and validation, so that extensibility is proven.
54. As an owner, I want every command to work with networking disabled, so that the tool does not depend on Logitech or any service.
55. As a script author, I want machine-readable JSON on stdout and progress/diagnostics on stderr, so that I can parse output without log noise.
56. As a script author, I want stable documented exit codes, so that I can branch on failure classes.
57. As an owner, I want a documented udev rule, so that I can run without root.
58. As an owner, I want all static assets embedded in the binary, so that the tool is self-contained.
59. As a maintainer, I want typed errors and no `unwrap`/`expect` in non-test code, so that failures are actionable.
60. As a maintainer, I want `tracing` spans with operation context, so that failures are diagnosable.
61. As a maintainer, I want the project GPLv3 and free of Logitech catalogue data, so that the licensing is clean.
62. As an owner, I want a clear recovery message naming the Backup after a failed Write, so that I know the next command.
63. As a maintainer, I want all behavior tested through one seam with a fake Remote, so that the suite runs without hardware.
64. As an owner, I want a physical acceptance checklist for the write path, so that the unverified reference sequence is proven on a real Harmony 300.

## Implementation Decisions

### Package and test seam

- One Cargo package with a library target and a binary target. Flat module tree: every source file is declared once at the crate root; no nested module directories and no `mod` blocks inside files. No workspace.
- The binary is a thin adapter: collect arguments, build the real runtime, call `run(args, runtime)`, and exit with its code.
- `run(args, runtime) -> Exit` is the single test seam. It owns argument parsing, command dispatch, validation, codecs, protocol sequencing, artifacts, JSON output, and exit-code mapping.
- `Runtime` supplies:
  - `RemoteBackend` for `list()` and `open(selector)`;
  - `Clock` for sidecar timestamps, Journal timestamps, and the Compose build stamp;
  - stdout and stderr writers;
  - a state directory for the Journal.
- `RemoteSession` exposes the low-level file protocol: reset, open-read, read, open-write, write-chunk, check, flush, close, reboot.
- Container and IR codecs are pure modules with public parse/emit/import functions. Table-driven tests call those public functions directly; no private helper is tested.

### Dependencies

- Runtime: `clap` 4.6.7 (derive), `serde` 1.0.229, `serde_json` 1.0.154, `sha2` 0.11.1, `thiserror` 2.0.21, `tracing` 0.1.44, `tracing-subscriber` 0.3.23 (fmt, no `env-filter`), `time` 0.3.55 (RFC3339 UTC), `hidapi` 2.6.7.
- Dev-only: `tempfile` 3.27.0, `jsonschema` (latest stable) for schema validation.
- Excluded: `anyhow`, `regex`, `rust-embed`, `dirs`, `schemars`, and any async runtime. Static assets use `include_str!`/`include_bytes!`; XDG paths are read directly.
- Re-check the latest stable versions when pinning; the versions above are the baseline as of 2026-11-05.

### CLI contract

- `detect [--json]`
- `backup --out <path> [--overwrite] [--json] [--print-schema]`
- `inspect --file <path> [--json] [--print-schema]`
- `compose --model <model.json> --out <container.bin> [--overwrite] [--json] [--print-schema] [--print-resolved-schema]`
- `write --container <path> --backup <path> [--commit] [--no-reboot] [--json]`
- `restore --backup <path> [--commit] [--no-reboot] [--json]`
- `write` and `restore` default to a Dry Run; `--commit` is required to open `/cfg/usercfg` for writing.
- `inspect --print-schema`, `backup --print-schema`, and `compose --print-schema` print their input/output schemas; `compose --print-resolved-schema` prints the resolved-model schema. They exit without requiring a file.
- `compose` and `backup` refuse to replace an existing output or sidecar unless `--overwrite` is supplied.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Internal or unexpected error |
| 2 | Usage error |
| 3 | Device error: not found, USB failure, unsupported model |
| 4 | Validation error: model, limits, Container size or checksum, Backup mismatch |
| 5 | Protocol error: refusal, malformed packet, short read, checksum check not `m` |
| 6 | Verification error: Read-Back mismatch |

JSON goes to stdout; progress and diagnostics go to stderr. Every documented code is exercised by an automated test.

### Artifacts

- Backup: the Container file plus `<backup-path>.json`, with `schema_version`, `file`, `model`, `skin`, `architecture`, `firmware_version`, `guid`, `serial`, `size`, `sha256`, `timestamp`, and `validation` (`ok`, `checksum_mismatch`, or `unreadable`).
- Compose: the Container plus `<container-path>.model.json`, the resolved model. Each Command records its IR Source format, resolved path, selector, source SHA-256, chosen protocol, `sends`, and a compact IR Record summary.
- Schemas: hand-written JSON Schema files for the model, inspect output, Backup sidecar, and resolved model, embedded with `include_str!` and exposed by the `--print-schema`/`--print-resolved-schema` flags.
- Journal: append-only JSONL at `$XDG_STATE_HOME/remote-rebirth/journal.jsonl`, falling back to `$HOME/.local/state/remote-rebirth/journal.jsonl`. Each record carries operation id, UTC timestamp, command, step, offset, byte count, reply status, Backup path, and Backup digest. Only committed `write` and `restore` operations journal.

### Detection

- Enumerate USB devices with vendor ID `0x046D`.
- The Harmony 300 and 350 share product ID `0xC124`. Resolve the family from `bcdDevice` (`0x1078` for the 300 family, `0x1104` for the 350 family), then read `/sys/sysinfo` for architecture, Skin, firmware version, hardware version, GUID, and serial number. Detect reports the family as well as the sysinfo identity.
- The Harmony 300 Model Profile is enabled for Architecture 16, skins 78 and 79, firmware 1.4.
- The Harmony 350 (Skin 104) is recognised and refused for all Writes. The Harmony 200 (`0xC125`) is recognised as unsupported. A single non-Harmony Logitech device is reported by vendor and product ID as unsupported; when a Harmony and another Logitech device are both attached, the Harmony is selected.
- The Remote must report `link_packet_length = 64`; any other value is refused before a Write.
- Detect opens read-only, reads `/sys/sysinfo`, and closes. It never writes.
- Multiple matching Remotes are an error listing their identities; the CLI does not choose one.
- No Remote is exit code 3 with a message naming the missing device.

### Container

- Architecture 16 frame: `GSPM` at offset 0, absolute `end_addr` at 4, a 15-entry section table at `0x0B`, `LWJL` at `0x47`, the key table at `0x4B`, a seeded checksum at `end−6`, and `PTYY` at `end−4`. Addresses are absolute 24-bit flash addresses with base `0x020000`.
- Checksum: 16-bit XOR of little-endian words, seed `0x4321`, over `[0, len−6)`. An odd trailing byte is not folded in.
- `/cfg/usercfg` reports `end_addr − 0x020000`, so a read omits the four-byte `PTYY` marker. Backup appends it; Write appends it to the Read-Back before comparison. A read that is not a recognisable Container is stored raw and marked unreadable.
- Maximum Container size is 256 KiB. Compose and Write reject anything larger before opening for writing.
- Compose emits from scratch. The layout, section order, dense numbering, empty skeleton, and `0xFF` key-map entry follow the MIT-licensed harmony-explorations codec, the only known blueprint that rebuilds Logitech compiles byte-for-byte and builds an empty Configuration.
- Every Configuration built from nothing binds firmware event `0xFF` to the action list called by the `0x30` entry. A Configuration that uses IR groups above 3 without that binding is rejected.
- The validator is stricter than the firmware: it checks the frame, checksum, section table bounds, slot bounds, IR group and record bounds, and Model Profile limits. It never guesses at unknown structures.
- Inspect exposes known sections plus `raw_slots[]` with index, offset, length, SHA-256, and `known`. Unknown slots are not parsed.

### File protocol and Write

- USB transport uses `hidapi` over hidraw: the kernel HID driver owns the interface, reports are 64 bytes, a write is a 65-byte buffer with a leading zero report ID that hidapi strips, and a read accepts 64 or 65 bytes with a 5 s timeout. The report size is confirmed against `/sys/sysinfo` and must be 64.
- Requests use the file protocol header, big-endian parameters, and a sequence counter that runs 0–63 and wraps. Data packets carry up to 62 bytes; a chunk carries at most 50 data packets and ends with the `0x7E` done packet. The done packet consumes no sequence number.
- Committed Write order: reset the filesystem, open `/cfg/usercfg` for writing, write chunks, request the checksum check, flush only when the reply is `m`, close, Read-Back, reboot unless `--no-reboot`.
- Open for writing erases the configuration region before replying. All preconditions and the Journal's prepare record must complete before the reset or open is sent.
- Preconditions: the Container parses as Architecture 16 with a present architecture record and valid checksum; its Skin matches the connected Remote; its Device count, IR-group count, and Favourite count fit the active Model Profile; the Backup exists and its sidecar matches; the Backup and Container model and Skin match the connected Remote; the Remote's current Configuration equals the Backup or is empty. Restore applies the same Container/Skin/profile checks and also refuses a sidecar whose validation is not `ok`, a deliberate safety extension.
- The checksum check uses type `XOR`, seed `0x4321`, offset 0, length `len−6`, and the Container's stored checksum word. A reply other than `m` prevents the flush and reports that the Remote may be erased.
- Read: the open reply carries the handle and the size; the first read report may repeat the request header; data packets carry sequence, size, and payload; the transfer ends on `0xFE`; the result is cut to the stated size. A short read is an error.
- A malformed packet poisons the session. Close the handle, reopen a fresh session, Journal the failure, and report. Do not retry the Write automatically.
- A Read-Back mismatch skips the reboot, exits 6, and names the Backup and the `restore --backup <path> --commit` recovery command.
- The CLI never opens firmware paths for writing. Only `/cfg/usercfg` is opened for writing.

### IR Sources

- Raw-first: timing-bearing IR Sources (Pronto `0000`/`0100`, LIRC `raw_codes`, Flipper `raw`) become class-1 pulse records directly. The record stores carrier period and on-time plus raw mark/space durations; no protocol name is stored.
- Protocol encoders are used only for IR Sources that carry protocol fields: Pronto shorthand, LIRC `codes`, and Flipper `parsed`.
- Six encoders are implemented from public timing facts: NEC, RC5, Sony SIRC (12/15/20), Kaseikyo/Panasonic, Samsung32, and JVC. The Logitech-derived rhythm table is never shipped.
- Pronto accepts `0000`, `0100`, `5000` (RC5), and `9000` (NEC). It rejects `5001`, `6000`, `7000`, `8000`, and `900A–900E` with the format word and Command name.
- LIRC accepts `raw_codes` and `SPACE_ENC`/`RC5` code blocks. It rejects RC6/RCMM/bi-phase, `CONST_LENGTH`, and repeat-mask configurations with the offending flag.
- Flipper accepts `raw` and parsed NEC/NECext/Samsung32/RC5/RC5X/SIRC/SIRC15/SIRC20/Kaseikyo. It rejects NEC42, RCA, and RC6.
- Press count defaults: NEC, Samsung32, JVC, and SIRC send 3; RC5 and Kaseikyo send 1. A Command may override this with `sends`. RC5 emits both toggle groups.
- Every rejection names the file, selector, protocol or flag, and Command.

### Model JSON

```json
{
  "schema_version": 1,
  "profile": "harmony_300",
  "skin": 78,
  "devices": [
    {
      "name": "TV",
      "device_key": 1,
      "commands": {
        "power":  { "source": { "format": "pronto", "hex": "0000 006D ..." }, "sends": 2 },
        "input":  { "source": { "format": "lirc", "file": "irs/tv.conf", "remote": "TV", "button": "Input" } },
        "vol_up": { "source": { "format": "flipper", "file": "irs/tv.ir", "command": "Vol_up" } }
      },
      "keys": { "power": "power", "volume_up": "vol_up", "input": "input" }
    }
  ],
  "activity": {
    "channel_device": "Cable",
    "steps": [
      { "device": "TV", "power": "power", "input": "input", "delay_ms": 500 },
      { "device": "Cable", "power": "power", "delay_ms": 500 }
    ]
  },
  "favourites": [
    { "key": 1, "channel": "101" },
    { "key": 2, "channel": "102" }
  ]
}
```

- `profile` and `skin` are required; Compose does not need a Remote.
- `device_key` is the key place `1..4`, not the printed label; the Model Profile supplies labels. Duplicates are rejected.
- `keys` maps canonical key names to Commands of the same Device. Unbound keys send nothing; cross-device punch-through is out of scope.
- Canonical key names: `power`, `volume_up`, `volume_down`, `volume_mute`, `channel_up`, `channel_down`, `number_0` through `number_9`, `direction_up`, `direction_down`, `direction_left`, `direction_right`, `select`, `menu`, `exit`, `info`, `guide`, `play`, `pause`, `stop`, `rewind`, `fast_forward`, `skip_back`, `skip_forward`, `record`, `red`, `green`, `yellow`, `blue`, `list`, `live`, `prev_channel`, `clear`, `enter`, `input`, `teletext`. Device keys, the Watch TV key, and Favourite keys are profile-owned and are not bindable through `keys`.
- Every declared Command must be referenced by `keys` or `activity`; unreferenced Commands are rejected so the Container stores only codes that are sent.
- `activity` is required and exactly one. `channel_device` names a declared Device. Steps run in order; `delay_ms` defaults to 500.
- `favourites` are optional. `key` is positional `1..capacity`; `channel` is 1–5 decimal digits, no leading zero, within the number sender's packed-decimal range.
- IR paths are relative to the model file; absolute paths are allowed. Inline Pronto hex is allowed.
- A Device may have an empty `keys` map only when its Commands are used by the activity.

### Model Profile

- A Model Profile declares product ID, architecture, accepted Skins, maximum Devices, maximum IR groups, Favourite capacity per Skin, key layout (canonical key name to scan code), and long-press capability.
- The Harmony 300 profile: product ID `0xC124`, Architecture 16, Skins 78 and 79, four Devices, four IR groups, Favourite capacity 5 on Skin 78 and 4 on Skin 79, short-press device keys only, long press disabled.
- The Harmony 350 profile is not enabled in v1. Detect recognises `bcdDevice 0x1104` and Skin 104 and refuses all Writes. A test registers a 350-like profile to prove Detect, Compose, and validation use it without changing Container parse or emit code.

### Errors, logging, licensing

- Errors are typed with `thiserror` and mapped to the exit-code table. Non-test code does not use `unwrap` or `expect`; errors surface actionable context to the CLI.
- `tracing` spans carry the operation and step; the subscriber writes to stderr. stdout is reserved for JSON or human command output.
- The project is GPLv3. Chorus and concordance logic may be ported with attribution; harmony-explorations prose and code are MIT. Logitech catalogue data, including the generated rhythm table, is never shipped or embedded.
- Static runtime assets are embedded: Model Profiles and JSON Schemas. The udev rule is documented in the repository and README; it is not a runtime asset.

## Testing Decisions

- A good test drives observable behavior through the single `run(args, runtime)` seam or a public codec function. It asserts stdout, stderr, exit code, and file bytes; it does not reach into private helpers or assert call order inside a module.
- The fake Remote is an in-memory implementation of `RemoteBackend` and `RemoteSession`. It records the exact request sequence and injects faults: refusal, malformed packet, short read, disconnect, checksum `u`, and Read-Back mismatch.
- The fixed `Clock` and a temporary state directory make Backup, Journal, and Compose output deterministic.
- Pure codec tests cover Container parse/emit round-trips, malformed frames, pointer and slot bounds, checksum, the 256 KiB limit, IR import, protocol encoders, press counts, toggle groups, and Model Profile limits.
- Golden Container fixtures are authored from the reference codec's documented empty Configuration and from this project's own Compose output. No Logitech catalogue data is used.
- Protocol-sequence tests assert chunk sizes, the 0–63 sequence wrap, the done packet, check parameters, flush only on `m`, close, Read-Back, reboot, and Journal records.
- Schema tests validate the model input, inspect output, Backup sidecar, and resolved-model output with `jsonschema` against the embedded schemas.
- CLI smoke tests cover `--help` and `--version` (exit 0, stdout), a usage error (exit 2, stderr), the `--print-schema` flags, and the exit-code taxonomy 1–6.
- Tests follow TDD/BDD: write the failing behavior first, name tests by scenario, action, and expected outcome, and keep them deterministic and isolated.
- The hardware gate is a manual checklist on the connected Harmony 300: detect identity, backup, inspect, compose, write dry run, write commit, Read-Back, restore, power-cycle, and IR bench verification. It is not part of CI.

## Out of Scope

- A GUI.
- Firmware reads or writes; only Configuration is written.
- Any network client, telemetry, or update check.
- Logitech's device catalogue or IR database.
- Enabling the Harmony 350 Model Profile; it is recognised and refused.
- Long press.
- More than one Watch TV Activity.
- Cross-device key bindings or volume punch-through.
- Favourite channels with leading zeros.
- RC6, RCMM, NEC42, RCA, Pronto `5001`/`6000`/`7000`/`8000`/`900A–900E`.
- Windows, macOS, or non-x86_64 Linux.
- Root or udev installation automation; the rule is documented only.
- Automatic retry after a failed Write; the operator re-runs after reviewing the Journal.
- Preserving the Remote's log area; the firmware's erase covers it.
- More than four Devices in v1; the profile table permits eight later.
- Localization.

## Further Notes

- The reference states that no Write, commit, or device-control sequence has ever been sent to a Harmony 300; the sequence is inferred from Logitech's client and one firmware reading. The physical gate is the only proof.
- The checksum check compares the stored trailer word. Whether the expected value must be sent big-endian is explicitly unchecked; the hardware gate covers it.
- Open for writing erases the configuration region before replying. A timeout there leaves the Remote erased; the Journal must record the prepare step before the reset.
- The reference also records a session-poisoning malformed packet and a 63-byte report wedge in hidapi. The chosen hidraw transport always presents a leading zero report ID and tolerates a 64- or 65-byte write; the fake Remote covers the poisoning behavior.
- The Harmony 300 and 350 share product ID `0xC124`; `bcdDevice` and `/sys/sysinfo` resolve the model and Skin. The Harmony 300's `bcdDevice` is `0x1078` even on Skin 79 units.
- A connected Harmony 300 is available on the development machine (`046D:C124`, `bcdDevice 0x1078`). Opening it requires the documented udev rule; until the rule is installed, `detect` exits 3 with a permission error and the hardware gate remains pending.
- Local tracker: this spec is `.scratch/remote-rebirth/spec.md`. No tickets have been created yet.
