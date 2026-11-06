# Remote Rebirth

A GPLv3 command-line tool that identifies a Logitech Harmony 300 over USB, backs up, composes, writes, and restores its configuration without network access.

## Language

**Remote**:
The connected Harmony 300 unit.
_Avoid_: device, unit

**Model**:
The versioned JSON declaration of devices, device-key placement, key maps, the Watch TV activity, favourites, and routing from which Compose builds a container.
_Avoid_: config, setup, project

**Model Profile**:
The declared capabilities and layout of one model family, used by detection, composition, and validation.
_Avoid_: profile, model

**Skin**:
A regional variant of a model that can change favourite capacity and key printing without changing the configuration format.
_Avoid_: region, variant

**Configuration**:
The logical content of a remote: devices, commands, key bindings, the Watch TV activity, and favourites.
_Avoid_: config, setup

**Container**:
The architecture-16 byte image written to `/cfg/usercfg`.
_Avoid_: image, blob, binary

**Backup**:
A container file plus its JSON sidecar, used as the recovery point before a write.
_Avoid_: dump, copy

**Project**:
The GUI-managed active Model plus its GUI-only state, backup library, and cached IR Library.
_Avoid_: workspace, config

**Device**:
A named appliance assigned to one device key, with named commands and key bindings.
_Avoid_: appliance, component

**Device Key**:
One of the four physical keys that selects a device; its place is the device's infrared group.
_Avoid_: device button, mode key

**Key Map**:
One device's mapping of canonical key names to its commands.
_Avoid_: button map, layout

**Routing**:
The model's assignment of the volume or channel function to a target device; Compose copies that device's key-map bindings into every other device's map.
_Avoid_: punch-through, volume lock

**Command**:
A named logical action a device can perform.
_Avoid_: button, function

**IR Record**:
The stored pulse record for one command.
_Avoid_: IR code, pulse code

**IR Source**:
A user-supplied file and selector that supplies one command's timings or protocol fields.
_Avoid_: IR file, code file

**IR Library**:
The locally cached set of open-format IR Sources that the GUI downloads or imports and searches by brand, model, and command.
_Avoid_: catalogue, database

**Compose**:
Build a container from a model, IR sources, and a target profile and skin, without a remote.
_Avoid_: compile, generate

**Write**:
Commit a container to the remote and verify it by read-back.
_Avoid_: flash, upload, sync

**Restore**:
Write a backup container back to the remote.
_Avoid_: rollback, recover

**Detect**:
Identify a connected remote without writing.
_Avoid_: probe, scan

**Watch TV Activity**:
The single activity bound to the Watch TV key; it toggles declared devices' power, selects their inputs, and selects the channel device.
_Avoid_: macro, scene

**Favourite**:
A channel number bound to a positional favourite key.
_Avoid_: preset

**Journal**:
The append-only record of acknowledged write steps used for recovery after a failed write.
_Avoid_: log

**Dry Run**:
Validation of a write or restore without opening the remote's configuration for writing.
_Avoid_: preview, test

**Prepare**:
The GUI action that validates, composes, detects, verifies, and Dry Runs a Write or Restore before a separate explicit Commit.
_Avoid_: preview, wizard

**Architecture 16**:
The file-based configuration architecture shared by the Harmony 300 and 350.
_Avoid_: arch16, GSPM

**Read-Back**:
The post-write read of the configuration compared with the written container.
_Avoid_: verify

"Flash" is reserved for firmware writes, which are out of scope; configuration writes are always called Write.
