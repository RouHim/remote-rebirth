//! Behaviour tests for the window: the Model edits, the safety gates, and the pages.
//!
//! Every test builds its App with [`App::at`], so the project, the backups and the journal all
//! live inside a temporary directory.

use std::collections::BTreeMap;
use std::path::PathBuf;

use iced::keyboard::{Key, Modifiers, key::Named};
use remote_rebirth_core::gui_api::{self, DetectReport, WriteResult};
use remote_rebirth_core::model::{Device, Routing};

use crate::app::App;
use crate::message::{
    CommandForm, Message, OpDone, PAGES, Page, Phase, PreparedKind, page_for_digit,
};
use crate::support::{form_source, list_backups, routed_target, scan_library};

/// A prepared-write report with no remote behind it.
fn prepared_write() -> WriteResult {
    WriteResult {
        operation: "write",
        commit: false,
        container: None,
        backup: PathBuf::from("backup.bin"),
        size: 0,
        backup_sha256: "0".repeat(64),
        bytes: 0,
        chunks: 0,
        data_packets: 0,
        reboot: false,
        journal: None,
    }
}

/// A supported detect report with the given firmware version.
fn detected_with_firmware(firmware: Option<&str>) -> DetectReport {
    DetectReport {
        schema_version: remote_rebirth_core::gui_api::DETECT_SCHEMA_VERSION,
        supported: true,
        model: "Harmony 300".into(),
        family: None,
        profile: Some("harmony_300"),
        architecture: Some(16),
        skin: Some(79),
        firmware_version: firmware.map(str::to_string),
        hardware_version: None,
        guid: None,
        serial: None,
        usb: remote_rebirth_core::gui_api::UsbIdentity {
            vendor_id: 0x046D,
            product_id: 0xC124,
            bcd_device: 0,
            product: None,
            serial: None,
            bus: 0,
            address: 0,
        },
    }
}

#[test]
fn library_scan_indexes_brand_model_and_commands() {
    let directory = tempfile::tempdir().expect("tempdir");
    let brand = directory.path().join("Samsung");
    std::fs::create_dir_all(&brand).expect("brand dir");
    std::fs::write(
            brand.join("TV.ir"),
            "Filetype: IR signals file\nVersion: 1\n#\nname: Power\ntype: parsed\nprotocol: NEC\naddress: 00 00 00 00\ncommand: 00 00 00 00\n#\nname: Volume Up\ntype: raw\nfrequency: 38000\ndata: 9000 4500 560 560\n",
        )
        .expect("write library file");
    let devices = scan_library(directory.path()).expect("scan");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].brand, "Samsung");
    assert_eq!(devices[0].model, "TV");
    assert_eq!(devices[0].commands, ["Power", "Volume Up"]);
}

#[test]
fn library_import_adds_all_commands_to_selected_device() {
    let directory = tempfile::tempdir().expect("tempdir");
    let brand = directory.path().join("Samsung");
    std::fs::create_dir_all(&brand).expect("brand dir");
    std::fs::write(
            brand.join("TV.ir"),
            "Filetype: IR signals file\nVersion: 1\nname: Power\ntype: parsed\nprotocol: NEC\naddress: 00 00 00 00\ncommand: 00 00 00 00\nname: Volume Up\ntype: raw\nfrequency: 38000\ndata: 9000 4500 560 560\n",
        )
        .expect("write library file");
    let (mut app, _) = App::at(directory.path().join("data"));
    app.library.devices = scan_library(directory.path()).expect("scan");
    app.library.selected = Some(0);
    app.project.model.devices.push(Device {
        name: "Living Room TV".into(),
        device_key: 1,
        commands: BTreeMap::new(),
        keys: BTreeMap::new(),
    });
    app.ui.selected_device = 0;
    let _ = app.update(Message::AddLibraryDevice);
    let device = &app.project.model.devices[0];
    assert!(device.commands.contains_key("Power"));
    assert!(device.commands.contains_key("Volume Up"));
    assert_eq!(device.commands["Power"].source.format, "flipper");
    assert_eq!(
        device.commands["Power"].source.command.as_deref(),
        Some("Power")
    );
    assert_eq!(device.commands["Power"].source.button, None);
}

#[test]
fn blank_app_builds_a_valid_one_device_project() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().to_path_buf());
    app.ui.new_device_name = "TV".into();
    app.ui.new_device_key = 1;
    app.add_device();
    app.ui.new_command.name = "power".into();
    app.ui.new_command.format = "pronto".into();
    app.ui.new_command.hex = "0000 006D 0002 0000 0020 0020 0020 0020".into();
    app.add_command();
    app.ui.selected_key = Some("power".into());
    let _ = app.update(Message::AssignCommand("power".into()));
    gui_api::validate_model(&app.project.model).expect("a valid one-Device project");
    assert!(directory.path().join("model.json").exists());
    assert!(directory.path().join("gui.json").exists());
}

#[test]
fn unreadable_model_is_kept_before_the_blank_project_can_be_saved() {
    let directory = tempfile::tempdir().expect("tempdir");
    let model_path = directory.path().join("model.json");
    std::fs::write(&model_path, b"{ not a Model }").expect("write an unreadable Model");
    let (app, _) = App::at(directory.path().to_path_buf());
    assert!(!model_path.exists(), "the unreadable file stays untouched");
    let kept: Vec<_> = std::fs::read_dir(directory.path())
        .expect("read_dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("model.json.unreadable-"))
        .collect();
    assert_eq!(kept.len(), 1, "the unreadable file was moved aside");
    assert!(app.ui.error.is_some(), "the loss is reported");
}

#[test]
fn source_form_requires_the_format_fields() {
    let mut form = CommandForm {
        format: "lirc".into(),
        file: "tv.conf".into(),
        ..CommandForm::default()
    };
    assert!(form_source(&form).is_err());
    form.remote = "TV".into();
    form.button = "Power".into();
    assert!(form_source(&form).is_ok());

    let mut flipper = CommandForm {
        format: "flipper".into(),
        file: "tv.ir".into(),
        ..CommandForm::default()
    };
    assert!(form_source(&flipper).is_err());
    flipper.button = "Power".into();
    assert!(form_source(&flipper).is_ok());
}

#[test]
fn sidebar_and_detect_buttons_emit_messages() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (app, _) = App::at(directory.path().to_path_buf());
    let mut ui = iced_test::simulator(app.view());
    ui.click("My Devices").expect("click My Devices");
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(matches!(
        messages.first(),
        Some(Message::Page(Page::Devices))
    ));

    let (app, _) = App::at(directory.path().to_path_buf());
    let mut ui = iced_test::simulator(app.view());
    ui.click("Find remote").expect("click Find remote");
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(matches!(messages.first(), Some(Message::Detect)));
}

#[test]
fn lifecycle_phase_follows_the_operation() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().to_path_buf());
    assert_eq!(app.operation.phase, Phase::Detect);
    app.begin("Backing up the Remote");
    assert_eq!(app.operation.phase, Phase::Backup);
    app.begin("Preparing a Write");
    assert_eq!(app.operation.phase, Phase::Prepare);
    app.begin("Writing to the Remote");
    assert_eq!(app.operation.phase, Phase::Commit);
    app.finish(OpDone::Committed(WriteResult {
        operation: "write",
        commit: true,
        container: None,
        backup: PathBuf::from("backup.bin"),
        size: 0,
        backup_sha256: "0".repeat(64),
        bytes: 0,
        chunks: 0,
        data_packets: 0,
        reboot: false,
        journal: None,
    }));
    assert_eq!(app.operation.phase, Phase::ReadBack);
    app.finish(OpDone::Detected(DetectReport {
        schema_version: remote_rebirth_core::gui_api::DETECT_SCHEMA_VERSION,
        supported: false,
        model: "Harmony 200".into(),
        family: None,
        profile: None,
        architecture: None,
        skin: None,
        firmware_version: None,
        hardware_version: None,
        guid: None,
        serial: None,
        usb: remote_rebirth_core::gui_api::UsbIdentity {
            vendor_id: 0,
            product_id: 0,
            bcd_device: 0,
            product: None,
            serial: None,
            bus: 0,
            address: 0,
        },
    }));
    assert_eq!(app.operation.phase, Phase::Detect);
}

#[test]
fn keyboard_shortcuts_change_chapters_and_dismiss() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().to_path_buf());
    assert_eq!(app.ui.page, Page::Remote);
    let _ = app.update(Message::Shortcut(
        Key::Character("3".into()),
        Modifiers::CTRL,
    ));
    assert_eq!(app.ui.page, Page::KeyMap);
    let _ = app.update(Message::Shortcut(
        Key::Named(Named::PageDown),
        Modifiers::CTRL,
    ));
    assert_eq!(app.ui.page, Page::Activities);
    let _ = app.update(Message::Shortcut(
        Key::Named(Named::PageUp),
        Modifiers::CTRL,
    ));
    assert_eq!(app.ui.page, Page::KeyMap);
    app.ui.error = Some("connection lost".into());
    let _ = app.update(Message::Shortcut(
        Key::Named(Named::Escape),
        Modifiers::NONE,
    ));
    assert!(app.ui.error.is_none());
}

#[test]
fn routing_is_reported_only_on_non_target_devices() {
    let mut model = gui_api::blank_model(79);
    model.routing = Some(Routing {
        volume: Some("TV".into()),
        channel: None,
    });
    assert_eq!(
        routed_target(&model, "volume_up", "Cable"),
        Some("TV".into())
    );
    assert_eq!(routed_target(&model, "volume_up", "TV"), None);
    assert_eq!(routed_target(&model, "channel_up", "Cable"), None);
}

#[test]
fn backup_paths_do_not_collide_in_one_clock_tick() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (app, _) = App::at(directory.path().join("data"));
    let first = app.backup_path_for("backup");
    let second = app.backup_path_for("backup");
    assert_ne!(first, second, "two operations must not share a backup name");
    assert_eq!(first.parent(), Some(app.project.backups_dir.as_path()));
    let name = first
        .file_name()
        .and_then(|name| name.to_str())
        .expect("name");
    assert!(name.starts_with("backup-"), "{name}");
    assert!(name.ends_with(".bin"), "{name}");
}

#[test]
fn backup_list_is_newest_first_across_prefixes() {
    let directory = tempfile::tempdir().expect("tempdir");
    for name in [
        "pre-write-100-0.bin",
        "backup-200-0.bin",
        "pre-write-300-0.bin",
    ] {
        std::fs::write(directory.path().join(name), b"").expect("write backup");
    }
    let names: Vec<String> = list_backups(directory.path())
        .iter()
        .map(|path| {
            path.file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        names,
        [
            "pre-write-300-0.bin",
            "backup-200-0.bin",
            "pre-write-100-0.bin"
        ],
        "the embedded timestamp decides, not the prefix"
    );
}

#[test]
fn text_edits_wait_for_the_autosave_tick() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().to_path_buf());
    app.ui.new_device_name = "TV".into();
    app.ui.new_device_key = 1;
    app.add_device();

    let _ = app.update(Message::DeviceName(0, "Television".into()));
    assert!(app.project.pending_save, "the edit waits for the tick");
    let before: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&app.project.model_path).expect("model file"))
            .expect("model json");
    assert_eq!(before["devices"][0]["name"], "TV");

    let _ = app.update(Message::Autosave);
    assert!(!app.project.pending_save);
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&app.project.model_path).expect("model file"))
            .expect("model json");
    assert_eq!(after["devices"][0]["name"], "Television");
}

#[test]
fn firmware_mismatch_is_reported_before_a_write() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().to_path_buf());
    app.project.remote = Some(detected_with_firmware(Some("9.9")));
    app.refresh_firmware_warning();
    assert!(
        app.ui
            .firmware_warning
            .as_deref()
            .unwrap_or_default()
            .contains("9.9"),
        "the warning names the detected version"
    );

    app.project.remote = Some(detected_with_firmware(Some("1.4")));
    app.refresh_firmware_warning();
    assert!(
        app.ui.firmware_warning.is_none(),
        "1.4 is the measured version"
    );
}

#[test]
fn the_journal_directory_is_injected_with_the_project() {
    let directory = tempfile::tempdir().expect("tempdir");
    let data = directory.path().join("data");
    let (app, _) = App::at(data.clone());
    assert_eq!(app.project.state_dir, data.join("state"));
    assert!(
        app.project.state_dir.is_dir(),
        "the journal directory exists before the first operation"
    );
    assert!(
        app.project.state_dir.starts_with(directory.path()),
        "tests never write into the shared state home"
    );
}

#[test]
fn commit_without_a_prepared_write_is_refused() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().join("data"));
    app.project.container_path = directory.path().join("container.bin");
    app.project.backup_path = Some(directory.path().join("backup.bin"));
    let _ = app.update(Message::CommitWrite);
    assert!(
        app.ui
            .error
            .as_deref()
            .is_some_and(|error| !error.is_empty()),
        "the commit explains what is missing"
    );
    assert!(!app.operation.busy, "no commit was started");
    assert!(!app.operation.committing);
    assert!(app.operation.last_prepare.is_none());
}

#[test]
fn a_changed_model_or_restore_path_drops_the_prepared_operation() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().join("data"));

    app.operation.prepared_kind = Some(PreparedKind::Write);
    app.operation.last_prepare = Some(prepared_write());
    let _ = app.update(Message::SkinChanged(78));
    assert_eq!(
        app.operation.prepared_kind, None,
        "a changed Model invalidates the prepared Write"
    );
    assert!(app.operation.last_prepare.is_none());

    app.operation.prepared_kind = Some(PreparedKind::Restore);
    app.operation.last_prepare = Some(prepared_write());
    app.ui.restore_path = "saved.bin".into();
    let _ = app.update(Message::RestorePath("other.bin".into()));
    assert_eq!(
        app.operation.prepared_kind, None,
        "a changed Restore path invalidates the prepared Restore"
    );
    assert!(app.operation.last_prepare.is_none());
}

#[test]
fn a_model_edit_drops_the_prepared_operation_before_the_tick() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().join("data"));
    app.ui.new_device_name = "TV".into();
    app.ui.new_device_key = 1;
    app.add_device();

    app.operation.prepared_kind = Some(PreparedKind::Write);
    app.operation.last_prepare = Some(prepared_write());
    app.ui.firmware_warning = Some("unverified".into());
    let _ = app.update(Message::DeviceName(0, "Television".into()));
    assert!(
        app.project.pending_save,
        "the edit still waits for the tick"
    );
    assert_eq!(
        app.operation.prepared_kind, None,
        "the prepared Write is dropped without waiting for the tick"
    );
    assert!(app.operation.last_prepare.is_none());
    assert!(
        app.ui.firmware_warning.is_none(),
        "the stale warning is dropped with the prepared operation"
    );
}

#[test]
fn preparing_a_write_flushes_the_pending_edit_first() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().join("data"));
    app.ui.new_device_name = "TV".into();
    app.ui.new_device_key = 1;
    app.add_device();
    let _ = app.update(Message::DeviceName(0, "Television".into()));
    assert!(app.project.pending_save, "the edit waits for the tick");

    let _ = app.update(Message::PrepareWrite);
    assert!(
        !app.project.pending_save,
        "the prepare flushed the edit before the busy guard dropped the tick"
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&app.project.model_path).expect("model file"))
            .expect("model json");
    assert_eq!(saved["devices"][0]["name"], "Television");
}

#[test]
fn preparing_a_restore_flushes_the_pending_edit_first() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().join("data"));
    app.ui.new_device_name = "TV".into();
    app.ui.new_device_key = 1;
    app.add_device();
    let backup = directory.path().join("saved.bin");
    std::fs::write(&backup, b"container").expect("write backup");
    app.ui.restore_path = backup.display().to_string();
    let _ = app.update(Message::DeviceName(0, "Television".into()));
    assert!(app.project.pending_save, "the edit waits for the tick");

    let _ = app.update(Message::PrepareRestore);
    assert!(
        !app.project.pending_save,
        "the prepare flushed the edit before the busy guard dropped the tick"
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&app.project.model_path).expect("model file"))
            .expect("model json");
    assert_eq!(saved["devices"][0]["name"], "Television");
}

#[test]
fn the_commit_lock_lasts_only_for_the_commit() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().join("data"));
    assert!(!app.operation.busy);
    assert!(!app.operation.committing);

    app.begin("Writing to the Remote");
    assert!(app.operation.busy, "a commit holds the window");
    assert!(!app.operation.committing, "the lock starts unlocked");
    app.operation.committing = true;

    let _ = app.update(Message::Page(Page::Settings));
    assert_eq!(
        app.ui.page,
        Page::Remote,
        "the busy guard ignores page changes during the commit"
    );

    let _ = app.update(Message::Done(Err("write failed".into())));
    assert!(!app.operation.busy);
    assert!(!app.operation.committing);
    let _ = app.update(Message::Page(Page::Settings));
    assert_eq!(
        app.ui.page,
        Page::Settings,
        "the window accepts messages again"
    );
}

#[test]
fn a_failed_write_offers_the_journal_and_a_restore() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut app, _) = App::at(directory.path().join("data"));
    let backup = app.project.backups_dir.join("pre-write-1.bin");
    std::fs::write(&backup, b"container").expect("write a backup file");
    std::fs::write(
        app.project.backups_dir.join("pre-write-1.bin.json"),
        br#"{
            "model": "Harmony 300",
            "skin": 79,
            "architecture": 16,
            "size": 9,
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "timestamp": "2026-11-05T12:00:00Z",
            "validation": "ok"
        }"#,
    )
    .expect("write the backup sidecar");
    app.project.backup_path = Some(backup.clone());
    app.ui.advanced = true;
    app.ui.error = Some("Read-back mismatch: the remote did not store it".into());

    let journal = app.project.state_dir.join("journal.jsonl");
    let mut ui = iced_test::simulator(app.view());
    ui.find("ERRATA").expect("the failure is printed as errata");
    ui.find("Activity log").expect("the journal label is shown");
    ui.find(journal.display().to_string().as_str())
        .expect("the journal path is shown");
    ui.click("Restore this backup")
        .expect("the restore control is enabled");
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::UseBackup(path) if path == &backup
        )),
        "clicking the control offers this Backup: {messages:?}"
    );
}

#[test]
fn every_chapter_has_one_shortcut_and_one_stored_index() {
    for (index, page) in PAGES.iter().enumerate() {
        assert_eq!(
            usize::from(page.index()),
            index,
            "stored index is the order"
        );
        assert_eq!(Page::from_index(index as u8), *page);
        let digit = (index + 1).to_string();
        assert_eq!(page_for_digit(&digit), Some(*page), "Ctrl+{digit}");
    }
    assert_eq!(
        Page::from_index(u8::MAX),
        Page::Remote,
        "unknown indices fall back"
    );
    assert_eq!(page_for_digit("0"), None);
    assert_eq!(page_for_digit("8"), None);
    assert_eq!(page_for_digit("12"), None);
}
