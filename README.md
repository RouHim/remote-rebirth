# Remote Rebirth

Offline configuration tool for the Logitech Harmony 300.

Remote Rebirth identifies a Harmony 300 over USB, backs up its Configuration, Composes a new
Container from a declarative JSON model plus locally imported IR Sources, and Writes or Restores the
Container with a checksum check, a Read-Back, and a Journal. It performs no network access and does
not depend on a Logitech account or service.

## Build

```sh
cargo build --release
```

The binary is `target/release/remote-rebirth`.

## USB permissions

The CLI needs access to the Remote's USB device node. Install the provided udev rule:

```sh
sudo install -m 0644 contrib/99-remote-rebirth.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

The rule covers the USB device and its `hidraw` node and grants the `wheel` group access, so it works
for local and headless/SSH sessions. Replace `wheel` with your group if needed. Reconnect the Remote
after installing the rule; for an already-connected unit, a software re-authorise (`echo 0/1` to
`/sys/bus/usb/devices/<port>/authorized`) re-enumerates it without physical access.

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

JSON goes to stdout; progress and diagnostics go to stderr.

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
