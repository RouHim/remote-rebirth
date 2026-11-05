# Hardware acceptance checklist

The reference documents no write, commit, or device-control sequence ever sent to a Harmony 300;
the sequence is inferred from Logitech's client and one firmware reading. Run this checklist on a real
Remote before treating the write path as proven.

## Prerequisites

- Harmony 300 connected by USB (`046D:C124`, `bcdDevice 0x1078` for the 300 family).
- udev rule installed from `contrib/99-remote-rebirth.rules`, or the CLI run with sufficient USB
  permissions.
- A bench IR receiver for the key-send checks.
- A known-good Backup made before any committed Write.

## Read-only checks

- [ ] `remote-rebirth detect --json` reports `supported: true`, `model: "Harmony 300"`,
      `architecture: 16`, the expected `skin` (78 or 79), firmware version, and GUID.
- [ ] `remote-rebirth backup --out /tmp/h300-backup.bin` writes the Container and sidecar.
- [ ] The sidecar `validation` is `ok`; `size` and `sha256` match the file.
- [ ] `remote-rebirth inspect --file /tmp/h300-backup.bin --json` parses without error and lists the
      Container metadata, Devices, key maps, Watch TV Activity, Favourites, and IR Records.

## Compose checks

- [ ] A model with four Devices, explicit Device Keys, a Watch TV Activity, and the profile's full
      Favourite capacity composes without error.
- [ ] `remote-rebirth inspect --file <composed.bin> --json` shows the declared Devices, keys,
      Activity, Favourites, and IR Records.
- [ ] `remote-rebirth write --container <composed.bin> --backup /tmp/h300-backup.bin` (Dry Run)
      succeeds without erasing the Remote.
- [ ] A model with duplicate Device Keys, an unsupported protocol, or too many Favourites is rejected
      before any write.

## Committed Write

- [ ] `remote-rebirth write --container <composed.bin> --backup /tmp/h300-backup.bin --commit`
      completes; the Read-Back equals the written Container.
- [ ] The Remote reboots (unless `--no-reboot`) and remains operational.
- [ ] Each Device Key selects its declared Device.
- [ ] Every configured key sends its expected code to the bench receiver.
- [ ] Watch TV switches the declared Devices on and selects their inputs.
- [ ] Each Favourite key sends its declared channel.

## Restore

- [ ] `remote-rebirth restore --backup /tmp/h300-backup.bin --commit` completes; the Read-Back equals
      the Backup byte for byte.
- [ ] The Remote remains operational after the restore.

## Refusals and recovery

- [ ] A Backup whose model or Skin differs from the connected Remote is refused before writing.
- [ ] A Write without a verified Backup is refused before writing.
- [ ] A Container with a bad checksum or over 256 KiB is refused before writing.
- [ ] A checksum check answering `u` prevents the flush and exits non-zero.
- [ ] A forced Read-Back mismatch exits 6, skips the reboot, names the Backup, and leaves the Backup
      intact.
- [ ] A disconnect mid-write journals the last acknowledged step and reports the recovery procedure.
- [ ] The Journal at `$XDG_STATE_HOME/remote-rebirth/journal.jsonl` contains the expected records.

## Record

Record the Remote's identity, firmware, skin, the Backup digest, the composed Container digest, and
the result of each check. Attach the Journal tail for any failure.
