# Hardware acceptance checklist

The reference documents no write, commit, or device-control sequence ever sent to a Harmony 300;
the sequence is inferred from Logitech's client and one firmware reading. Run this checklist on a real
Remote before treating the write path as proven.

## Prerequisites

- Harmony 300 connected by USB (`046D:C124`, `bcdDevice 0x1078` for the 300 family).
- udev rule installed from `contrib/99-remote-rebirth.rules`, or the CLI run with sufficient USB
  permissions.
- A bench IR receiver for the key-send checks.
- A known-good Backup made before any committed Write, outside `/tmp`; `/tmp` is cleared and is not a
  recovery point.

## Read-only checks

- [x] `remote-rebirth detect --json` reports `supported: true`, `model: "Harmony 300"`,
      `architecture: 16`, the expected `skin` (78 or 79), firmware version, and GUID.
- [x] `remote-rebirth backup --out $HOME/h300-backups/h300-backup.bin` writes the Container and sidecar.
- [x] The sidecar `validation` is `ok`; `size` and `sha256` match the file.
- [x] `remote-rebirth inspect --file $HOME/h300-backups/h300-backup.bin --json` parses without error and lists the
      Container metadata, Devices, key maps, Watch TV Activity, Favourites, and IR Records.

## Compose checks

- [ ] A model with four Devices, explicit Device Keys, a Watch TV Activity, and the profile's full
      Favourite capacity composes without error.
- [x] `remote-rebirth inspect --file <composed.bin> --json` shows the declared Devices, keys,
      Activity, Favourites, and IR Records.
- [x] `remote-rebirth write --container <composed.bin> --backup $HOME/h300-backups/h300-backup.bin` (Dry Run)
      succeeds without erasing the Remote.
- [ ] A model with duplicate Device Keys, an unsupported protocol, or too many Favourites is rejected
      before any write.

## Committed Write

- [x] `remote-rebirth write --container <composed.bin> --backup $HOME/h300-backups/h300-backup.bin --commit`
      completes; the Read-Back equals the written Container.
- [x] The Remote reboots (unless `--no-reboot`) and remains operational.
- [ ] Each Device Key selects its declared Device.
- [ ] Every configured key sends its expected code to the bench receiver.
- [ ] Watch TV switches the declared Devices on and selects their inputs.
- [ ] Each Favourite key sends its declared channel.

## Restore

- [x] `remote-rebirth restore --backup $HOME/h300-backups/h300-backup.bin --commit` completes; the Read-Back equals
      the Backup byte for byte.
- [x] The Remote remains operational after the restore.

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

## Verification record

Date: 2026-11-05. Unit: Harmony 300, USB `046D:C124`, `bcdDevice 0x1078`, skin 79, architecture 16,
firmware 1.4.0, hardware 0.0. The original Configuration was 24725 bytes, SHA-256
`d1b78d77dacf1e1c2dac124bb3b4a755d5a6d76bcc9b772e169fc73d9cbfa279`.

- `detect`: passed, exit 0, `supported: true`, `model: "Harmony 300"`, skin 79.
- `backup`: passed, 24725 bytes, sidecar `validation: ok`.
- `inspect`: passed, parsed 15 slots, 1 Device, 0 Favourites.
- `compose`: passed, 803 bytes, 1 Device, SHA-256
  `1dc12d045d05534444f0c9e4c4962b1f67f76d0300c7b98784a8c37819f81a72`.
- A four-Device, four-Favourite model composed to 1893 bytes, SHA-256
  `87ec5f371ac59c53975bf5cff7bc02cc338f9bca829419fbb5048d58301c8d15`; `inspect` showed the four
  Device Keys and four Favourites, and the `write` Dry Run passed (1 chunk, 31 data packets).
- `write` Dry Run: passed, 803 bytes, 1 chunk, 13 data packets.
- Committed `write`: first attempt failed at the chunk acknowledgement because the expected byte was
  read at offset 0 instead of offset 2; the open had erased the Configuration. `restore` of the
  24725-byte Backup recovered the unit and verified by Read-Back. After the correction the committed
  `write` completed, verified its Read-Back, and rebooted. An independent `backup` of the written
  Configuration was byte-identical to the composed Container.
- `restore` of the original: completed, verified by Read-Back, and rebooted. An independent `backup`
  was byte-identical to the original 24725-byte Backup.
- The Remote re-enumerated after each reboot; a `backup` run during re-enumeration failed with "HID
  device ... no longer connected", and the same command passed after four seconds.

Not covered by this run: a physical IR bench (the verification model used synthetic Pronto pulses), a
four-Device/full-Favourite committed Write, and the refusal cases that need induced protocol faults.
The write, chunked-write, Read-Back, reboot, and Restore paths were exercised on hardware.

The original Configuration is preserved outside `/tmp` at
`/home/rouven/h300-backups/2026-11-05-original.bin` with its sidecar; its SHA-256 is the original
`d1b78d77dacf1e1c2dac124bb3b4a755d5a6d76bcc9b772e169fc73d9cbfa279`.
