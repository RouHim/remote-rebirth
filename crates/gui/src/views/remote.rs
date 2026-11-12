//! The Remote page: identity, backups, and the write/restore path.

use super::widgets::{
    backup_row, body, danger, data_line, errata, ghost, note, notice, paint, primary, procedure,
    secondary, section, stamp,
};
use crate::app::App;
use crate::message::{Message, Phase, PreparedKind};
use crate::support::{plain_number, short_digest};
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{checkbox, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Length};

pub(super) fn remote_page(app: &App) -> Element<'_, Message> {
    let current_backup: Element<'_, Message> = if app.ui.advanced {
        match &app.project.backup_path {
            Some(path) => data_line("Current backup", &path.display().to_string()),
            None => iced::widget::Space::new().height(0).into(),
        }
    } else {
        iced::widget::Space::new().height(0).into()
    };
    let restore_field: Element<'_, Message> = if app.ui.advanced {
        text_input("Path to backup file", &app.ui.restore_path)
            .on_input(Message::RestorePath)
            .padding(6)
            .size(11)
            .font(t::mono(Weight::Normal))
            .style(t::text_field)
            .into()
    } else if app.ui.restore_path.trim().is_empty() {
        note("Choose a backup from the list first.")
    } else {
        iced::widget::Space::new().height(0).into()
    };
    let left = column![
        section(
            "Your remote",
            Some("IDENTITY"),
            column![identity_block(app), version_note(app)]
                .spacing(10)
                .into(),
        ),
        section(
            "Backup",
            Some("RECOVERY POINT"),
            column![
                body("Save the remote's current setup before changing anything."),
                row![
                    secondary("Find remote", Message::Detect, app.operation.busy),
                    secondary("Save backup", Message::Backup, app.operation.busy),
                    ghost("Refresh", Message::RefreshBackups, app.operation.busy),
                ]
                .spacing(6),
                current_backup,
                backup_list(app),
            ]
            .spacing(10)
            .into(),
        ),
        section(
            "Before you write",
            Some("SAFETY"),
            column![
                row![
                    text("01")
                        .font(t::mono(Weight::Normal))
                        .size(8)
                        .color(t::OXIDE)
                        .width(Length::Fixed(22.0)),
                    body("A backup is saved before every update."),
                ]
                .spacing(6),
                row![
                    text("02")
                        .font(t::mono(Weight::Normal))
                        .size(8)
                        .color(t::OXIDE)
                        .width(Length::Fixed(22.0)),
                    body("The remote is read back after writing."),
                ]
                .spacing(6),
                row![
                    text("03")
                        .font(t::mono(Weight::Normal))
                        .size(8)
                        .color(t::OXIDE)
                        .width(Length::Fixed(22.0)),
                    body("If the check fails, restore the backup."),
                ]
                .spacing(6),
            ]
            .spacing(6)
            .into(),
        ),
    ]
    .spacing(18)
    .width(Fill);

    let right = column![
        if let Some(error) = &app.ui.error {
            errata(app, error)
        } else {
            iced::widget::Space::new().height(0).into()
        },
        if let Some(warning) = &app.ui.firmware_warning {
            notice(
                "Unverified firmware",
                warning,
                Message::DismissFirmwareWarning,
            )
        } else {
            iced::widget::Space::new().height(0).into()
        },
        section(
            "Update remote",
            Some("WRITE"),
            column![
                procedure(
                    "1",
                    "Check the setup",
                    "Build the new setup and check it against the remote.",
                    secondary(
                        "Check setup",
                        Message::Compose,
                        app.operation.busy || app.validation().is_some()
                    ),
                ),
                procedure(
                    "2",
                    "Review the update",
                    "A backup is saved first. You see exactly what will be written.",
                    secondary(
                        "Review update",
                        Message::PrepareWrite,
                        app.operation.busy || app.validation().is_some()
                    ),
                ),
                procedure(
                    "3",
                    "Write to the remote",
                    "The remote is erased, written, checked and read back.",
                    primary(
                        "Update remote",
                        Message::CommitWrite,
                        app.operation.busy
                            || app.validation().is_some()
                            || app.operation.prepared_kind != Some(PreparedKind::Write)
                    ),
                ),
                update_summary(app),
                checkbox(app.ui.reboot)
                    .label("Restart the remote after the update")
                    .text_size(11.5)
                    .font(t::sans(Weight::Normal))
                    .on_toggle(Message::Reboot)
                    .style(t::checkbox),
            ]
            .spacing(10)
            .into(),
        ),
        section(
            "Restore a backup",
            Some("READ-BACK"),
            column![
                body("Put a saved backup back on the remote. A damaged backup is refused."),
                restore_field,
                row![
                    secondary(
                        "Review restore",
                        Message::PrepareRestore,
                        app.operation.busy
                    ),
                    primary(
                        "Restore remote",
                        Message::CommitRestore,
                        app.operation.busy
                            || app.operation.prepared_kind != Some(PreparedKind::Restore)
                    ),
                ]
                .spacing(6),
            ]
            .spacing(10)
            .into(),
        ),
    ]
    .spacing(18)
    .width(Fill);

    row![
        scrollable(left).height(Fill).width(Length::FillPortion(2)),
        container(iced::widget::Space::new())
            .width(Length::Fixed(1.0))
            .height(Fill)
            .style(|_| container::Style {
                background: Some(paint(t::RULE)),
                ..Default::default()
            }),
        scrollable(right).height(Fill).width(Length::FillPortion(3)),
        phase_rail(app),
    ]
    .spacing(18)
    .height(Fill)
    .padding([14, 24])
    .into()
}

pub(super) fn identity_block(app: &App) -> Element<'_, Message> {
    match &app.project.remote {
        Some(remote) if app.ui.advanced => column![
            text(&remote.model).font(t::serif()).size(20).color(t::INK),
            data_line("Family", remote.family.unwrap_or("unknown")),
            data_line("Architecture", &plain_number(remote.architecture)),
            data_line("Skin", &plain_number(remote.skin)),
            data_line(
                "Firmware",
                remote.firmware_version.as_deref().unwrap_or("unknown")
            ),
            data_line(
                "Hardware",
                remote.hardware_version.as_deref().unwrap_or("unknown")
            ),
            data_line("GUID", remote.guid.as_deref().unwrap_or("unknown")),
            data_line("Serial", remote.serial.as_deref().unwrap_or("unknown")),
            data_line(
                "USB",
                &format!(
                    "{} · {:04X}:{:04X} · bus {} address {}",
                    remote.usb.product.as_deref().unwrap_or("Logitech"),
                    remote.usb.vendor_id,
                    remote.usb.product_id,
                    remote.usb.bus,
                    remote.usb.address
                )
            ),
        ]
        .spacing(4)
        .into(),
        Some(remote) => column![
            text(&remote.model).font(t::serif()).size(20).color(t::INK),
            body("Connected and ready."),
            data_line(
                "Firmware",
                remote.firmware_version.as_deref().unwrap_or("unknown")
            ),
        ]
        .spacing(5)
        .into(),
        None => column![body(
            "Connect the Harmony 300 with the USB cable, then choose Find remote."
        ),]
        .spacing(8)
        .into(),
    }
}

pub(super) fn version_note(app: &App) -> Element<'_, Message> {
    if let Some(remote) = &app.project.remote
        && let Some(skin) = remote.skin
        && skin != app.project.model.skin
    {
        return column![
            note(if app.ui.advanced {
                format!(
                    "The connected remote reports Skin {skin}; the setup uses Skin {}.",
                    app.project.model.skin
                )
            } else {
                "This setup was made for a different remote version.".into()
            }),
            secondary(
                "Use the connected remote's version",
                Message::UseDetectedSkin(skin),
                app.operation.busy
            ),
        ]
        .spacing(6)
        .into();
    }
    iced::widget::Space::new().height(0).into()
}

pub(super) fn backup_list(app: &App) -> Element<'_, Message> {
    if app.project.backups.is_empty() {
        return note("No backups saved yet.");
    }
    let mut rows: Vec<Element<'_, Message>> = vec![
        row![
            text("№")
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(t::MUTED)
                .width(Length::Fixed(24.0)),
            text("STATE")
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(t::MUTED)
                .width(Length::Fixed(74.0)),
            text("DETAIL")
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(t::MUTED),
            iced::widget::Space::new().width(Fill),
        ]
        .spacing(6)
        .into(),
    ];
    for (index, path) in app.project.backups.iter().take(6).enumerate() {
        let (detail, validation) = backup_row(path, app.ui.advanced);
        let verified = validation == "ok";
        let stamp = backup_stamp(&validation);
        rows.push(
            column![
                row![
                    text(format!("{:02}", index + 1))
                        .font(t::mono(Weight::Normal))
                        .size(9)
                        .color(t::FAINT)
                        .width(Length::Fixed(24.0)),
                    stamp,
                    text(detail)
                        .font(t::mono(Weight::Normal))
                        .size(8.5)
                        .color(t::MUTED)
                        .width(Fill),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                row![
                    iced::widget::Space::new().width(Fill),
                    ghost(
                        "Use for restore",
                        Message::UseBackup(path.clone()),
                        app.operation.busy || !verified
                    ),
                    danger(
                        "Remove",
                        Message::DeleteBackup(path.clone()),
                        app.operation.busy
                    ),
                ]
                .spacing(4),
            ]
            .spacing(3)
            .into(),
        );
        rows.push(rule::horizontal(1.0).style(t::rule_style).into());
    }
    column(rows).spacing(6).into()
}

pub(super) fn backup_stamp(validation: &str) -> Element<'static, Message> {
    match validation {
        "ok" => stamp("Verified", t::GRASS),
        "checksum_mismatch" | "unreadable" => stamp("Invalid", t::OXIDE_DARK),
        _ => stamp("Unchecked", t::FAINT),
    }
}

pub(super) fn update_summary(app: &App) -> Element<'_, Message> {
    let prepared = app
        .operation
        .last_prepare
        .as_ref()
        .filter(|_| app.operation.prepared_kind == Some(PreparedKind::Write))
        .map(|report| {
            if app.ui.advanced {
                format!(
                    "Ready — {} bytes, {} chunks, {} packets, backup {}, reboot {}.",
                    report.bytes,
                    report.chunks,
                    report.data_packets,
                    short_digest(&report.backup_sha256),
                    if report.reboot { "yes" } else { "no" }
                )
            } else {
                "Ready to update. A backup was saved first.".to_string()
            }
        });
    match prepared {
        Some(summary) => note(summary),
        None => note(
            "Check the setup, then review the update. Nothing is written until you press Update remote.",
        ),
    }
}

pub(super) fn phase_rail(app: &App) -> Element<'_, Message> {
    let current = match app.operation.phase {
        Phase::Detect => "DETECT",
        Phase::Backup => "BACKUP",
        Phase::Prepare => "PREPARE",
        Phase::Commit => "COMMIT",
        Phase::ReadBack => "READBACK",
    };
    let phases = [
        ("01", "DETECT", 2u16),
        ("02", "BACKUP", 3),
        ("03", "PREPARE", 3),
        ("04", "COMMIT", 2),
        ("05", "READBACK", 2),
    ];
    let mut cells: Vec<Element<'_, Message>> = Vec::new();
    for (number, name, portion) in phases {
        let active = name == current;
        cells.push(
            container(
                column![
                    text(number)
                        .font(t::mono(Weight::Normal))
                        .size(8)
                        .color(if active { t::OXIDE_DARK } else { t::FAINT }),
                    text(name)
                        .font(t::mono(Weight::Medium))
                        .size(8)
                        .color(if active { t::INK } else { t::MUTED }),
                    iced::widget::Space::new().height(Fill),
                    container(iced::widget::Space::new())
                        .width(Length::Fixed(7.0))
                        .height(Length::Fixed(7.0))
                        .style(move |_| container::Style {
                            background: if active { Some(paint(t::OXIDE)) } else { None },
                            border: iced::Border {
                                width: 1.0,
                                color: if active { t::OXIDE } else { t::RULE },
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                ]
                .spacing(4),
            )
            .width(Fill)
            .height(Length::FillPortion(portion))
            .padding([8, 7])
            .style(move |_| container::Style {
                background: if active {
                    Some(paint(t::OXIDE_SOFT))
                } else {
                    None
                },
                border: iced::Border {
                    width: 1.0,
                    color: if active { t::OXIDE } else { t::RULE },
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into(),
        );
    }
    container(column(cells).spacing(0))
        .width(Length::Fixed(74.0))
        .height(Fill)
        .padding([0, 7])
        .into()
}

// -- devices ----------------------------------------------------------------
