# Product

<!-- impeccable:product-schema 1 -->

## Platform

linux

## Stack

Rust workspace with a native Iced 0.14 desktop application (`crates/gui`) on Linux x86_64. It links
`remote-rebirth-core` directly; the CLI is a separate binary over the same core. No web layer, no
embedded browser.

## Users

Owners of a Logitech Harmony 300 who used the original desktop software and expect to add a device,
pick commands, click keys on a picture of the remote and press Update Remote. They are regular
households, not technicians: they do not know canonical key names, selectors, container layouts or
JSON. They configure one remote occasionally — on first setup, after replacing a television, or when
recovering from a failed change — usually with the remote connected by USB and often on a machine
without working network access. The CLI exists for scripting and recovery, not as the GUI user's
surface.

## Product Purpose

An offline configuration tool that detects the connected Harmony 300, backs up its Configuration,
lets the owner edit devices, commands, key maps, volume/channel routing, the Watch TV activity and
favourites, then composes, prepares and explicitly commits a Container write with a read-back and a
restore path. Success means a regular owner can finish a working setup without writing JSON, without
knowing internal key names, and without an account, catalogue download or network request.

## Positioning

The only configuration path for this remote that works with no Logitech catalogue, account, service
or network dependency: the Container is built locally from user-supplied IR data, every write is
gated by a verified Backup and a Read-Back, and the GUI shares one core with a stable CLI instead of
reimplementing its safety rules.

## Operating Context

The Remote sits on the desk, connected over USB, while the owner edits on a Linux desktop. The
window is used at 1240×820 by default and must not clip at the 1040×680 minimum. Editing is an
occasional task measured in minutes, not a daily workspace, and is often interrupted by plugging the
remote in or out. The machine may have networking disabled; the GUI must remain fully usable from a
cached or locally imported IR Library. Backups are the owner's recovery point and stay visible and
manageable. The CLI remains the scripting and recovery surface with unchanged JSON and exit codes.
The GUI is English-only for v1; user-facing strings stay isolated for later translation.

## Capabilities and Constraints

Implemented first slice: Detect and identity, managed Backup library, Devices and Commands (local
file or inline Pronto), Key Map with schematic buttons, mirrored table and command palette,
volume/channel Routing, Compose, Prepare/Commit Write, Read-Back, Prepare/Commit Restore, Watch TV
activity, Favourites, and a Settings page. The optional pinned Flipper-IRDB download and browser,
drag-and-drop assignment and field-level inline validation are specified but not yet implemented.

Hard constraints: the core and CLI contain no network client; the GUI may only download the IR
Library after explicit owner consent. No firmware reads or writes, no IR learning or capture, no
Logitech catalogue or proprietary IR data, no more than one Watch TV activity, no long press, no
routing for digits/Enter/previous channel/Input, no favourite leading zeros. The Container is limited
to 256 KiB. The core's safety gates are never bypassed: verified Backup, model/Skin match, current
Configuration equal to the Backup or empty, size and checksum, Read-Back equality, reboot policy.
Cancel is disabled once a commit starts.

## Brand Commitments

The product name is Remote Rebirth. It is GPL-3.0-or-later. The voice is plain, friendly and
non-technical, written for regular Harmony owners (technical details behind an opt-in "Show technical
details" setting). The product is offline-first and must never imply telemetry, accounts, update
checks or cloud dependence. It must not imitate Logitech's software, trademarks, catalogue or visual
identity.

## Evidence on Hand

- `.scratch/remote-rebirth-gui/screenshots/` — the shipped owner's-manual GUI at 1240×820 and
  1040×680, plus `first-slice/` with the superseded 960×1080 captures of the first slice.
- `.scratch/remote-rebirth-gui/spec.md` — GUI scope, user stories, decisions and implementation
  status.
- `.scratch/remote-rebirth/spec.md` — core and CLI contract, including detection, compose, write and
  restore semantics.
- `GLOSSARY.md` — canonical domain terms (Remote, Model, Skin, Container, Backup, Device, Device Key,
  Key Map, Routing, Command, IR Record).
- `docs/adr/0001`–`0009` — hardware and offline constraints.
- `docs/hardware-acceptance.md` — physical acceptance gate.

There are no real customer testimonials, logos, screenshots of Logitech software, licensed
photography or benchmark claims. Do not fabricate them. The remote is a physical object the owner
already has; no product photography ships with the application.

## Product Principles

1. The connected Remote is the authority: detect its identity, lock the matching Skin, and verify
   before and after every write.
2. Never a wall of schema errors: plain-language, field-level feedback arrives before Compose can
   fail.
3. Reversible by default: an automatic Backup precedes every write, Commit is separate and explicit,
   Read-Back proves the result, and Restore is one action away.
4. Offline is a feature, not a limitation: no surprise request, no telemetry, no background update.
5. One core, two surfaces: the GUI borrows the CLI's gates instead of growing a second, subtly
   different implementation.

## Accessibility & Inclusion

Every action must be reachable by keyboard, with assignment parity between mouse and keyboard.
Focus is always visible; no state is conveyed by colour alone. The 1040×680 minimum window must not
clip controls. Labels and errors use plain language rather than internal key names or protocol terms,
which stay behind the technical-details setting. Text contrast targets WCAG AA against the committed
palette.
