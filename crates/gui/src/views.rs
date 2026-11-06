//! The pages of the owner's manual.
//!
//! The shell is a cloth rail and a paper sheet. Pages are set as spreads:
//! headings in the serif, work text in the sans, codes and counts in the mono,
//! and rules instead of cards. The one repeating signature is the numbered key
//! drawing whose callouts match the work table.

use iced::font::Weight;
use iced::widget::{
    Column, Row, button, checkbox, column, container, pick_list, progress_bar, row, rule,
    scrollable, text, text_input,
};
use iced::{Alignment, Color, Element, Fill, Length};

use super::theme as t;
use super::{
    App, CommandForm, Message, Page, Phase, PreparedKind, device_key_from_label, device_key_label,
    device_key_labels, humanize, none_if_placeholder, plain_number, profile, routed_target,
    short_digest, step_label,
};

const KEY_COLUMNS: usize = 3;
const NO_COMMAND: &str = "—";

pub fn view(app: &App) -> Element<'_, Message> {
    let sheet = container(column![
        page_header(app),
        rule::horizontal(1.0).style(t::ink_rule),
        page_body(app),
        rule::horizontal(1.0).style(t::rule_style),
        sheet_footer(app),
    ])
    .width(Fill)
    .height(Fill)
    .style(t::sheet);

    let workspace = column![
        container(sheet).padding(14).height(Fill).style(t::desk),
        status_bar(app),
    ]
    .height(Fill);

    row![rail(app), workspace].height(Fill).into()
}

fn page_header(app: &App) -> Element<'_, Message> {
    let (title, subtitle) = match app.page {
        Page::Remote => (
            "Remote",
            "Your Harmony 300, its backup, and the update path.",
        ),
        Page::Devices => (
            "My Devices",
            "Name each device and give it the commands you need.",
        ),
        Page::KeyMap => (
            "Buttons",
            "Find the key in the drawing, then give it a command.",
        ),
        Page::Activities => ("Watch TV", "One button switches everything on in order."),
        Page::Favourites => ("Favourites", "The channel buttons you press most."),
        Page::Settings => ("Settings", "Display, files, and what this program is."),
    };
    let section = match app.page {
        Page::Remote => "1",
        Page::Devices => "2",
        Page::KeyMap => "3",
        Page::Activities => "4",
        Page::Favourites => "5",
        Page::Settings => "6",
    };
    let place = "Living Room · Harmony 300".to_string();
    let invalid = app.validation().is_some();
    row![
        column![
            text(title).font(t::serif()).size(31).color(t::INK),
            text(subtitle)
                .font(t::sans(Weight::Normal))
                .size(12)
                .color(t::MUTED),
        ]
        .spacing(4)
        .width(Fill),
        column![
            text(format!("SECTION {section}"))
                .font(t::mono(Weight::Normal))
                .size(8.5)
                .color(t::FAINT),
            text(place)
                .font(t::mono(Weight::Normal))
                .size(9.5)
                .color(t::MUTED),
        ]
        .align_x(Alignment::End)
        .spacing(2),
        row![
            secondary("Back up now", Message::Backup, app.busy),
            primary("Review update", Message::PrepareWrite, app.busy || invalid),
        ]
        .spacing(8),
    ]
    .spacing(18)
    .align_y(Alignment::Start)
    .padding([18, 24])
    .into()
}

fn page_body(app: &App) -> Element<'_, Message> {
    match app.page {
        Page::Remote => remote_page(app),
        Page::Devices => devices_page(app),
        Page::KeyMap => keymap_page(app),
        Page::Activities => activities_page(app),
        Page::Favourites => favourites_page(app),
        Page::Settings => settings_page(app),
    }
}

fn sheet_footer(app: &App) -> Element<'_, Message> {
    let page_number = match app.page {
        Page::Remote => "12",
        Page::Devices => "24",
        Page::KeyMap => "42",
        Page::Activities => "58",
        Page::Favourites => "66",
        Page::Settings => "78",
    };
    let saved = if app.saved {
        "All changes saved"
    } else {
        "Unsaved changes"
    };
    let container_size = app
        .container_size
        .map(|size| format!(" · Container {} B of 262144", size))
        .unwrap_or_default();
    container(
        row![
            text("REMOTE REBIRTH · LIVING ROOM · HARMONY 300")
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(t::FAINT),
            iced::widget::Space::new().width(Fill),
            text(format!("{saved}{container_size}"))
                .font(t::mono(Weight::Normal))
                .size(8.5)
                .color(t::MUTED),
            iced::widget::Space::new().width(Fill),
            text(page_number)
                .font(t::mono(Weight::Normal))
                .size(8.5)
                .color(t::FAINT),
        ]
        .align_y(Alignment::Center)
        .padding([6, 24]),
    )
    .into()
}

fn status_bar(app: &App) -> Element<'_, Message> {
    let identity = match &app.remote {
        Some(remote) if app.advanced => format!(
            "{} · Skin {} · firmware {}",
            remote.model,
            plain_number(remote.skin),
            remote.firmware_version.as_deref().unwrap_or("?")
        ),
        Some(remote) => format!("{} connected", remote.model),
        None => "No remote connected".to_string(),
    };
    let validation = match (app.advanced, app.validation()) {
        (true, Some(error)) => error,
        (false, Some(_)) => "Setup incomplete".to_string(),
        (_, None) => "Ready".to_string(),
    };
    let saved = if app.saved {
        "All changes saved"
    } else {
        "Unsaved changes"
    };
    let mut items = row![
        dot(if app.remote.is_some() {
            t::GRASS
        } else {
            t::FAINT
        }),
        text(identity)
            .font(t::mono(Weight::Normal))
            .size(9.5)
            .color(t::INK),
        text("·")
            .font(t::mono(Weight::Normal))
            .size(9.5)
            .color(t::FAINT),
        text(saved)
            .font(t::mono(Weight::Normal))
            .size(9.5)
            .color(t::MUTED),
        text("·")
            .font(t::mono(Weight::Normal))
            .size(9.5)
            .color(t::FAINT),
        text(validation)
            .font(t::mono(Weight::Normal))
            .size(9.5)
            .color(if app.validation().is_some() {
                t::OXIDE_DARK
            } else {
                t::GRASS
            }),
        iced::widget::Space::new().width(Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    if let Some((label, fraction)) = &app.progress {
        items = items.push(
            progress_bar(0.0..=1.0, *fraction)
                .girth(5.0)
                .length(Length::Fixed(150.0))
                .style(t::progress),
        );
        items = items.push(
            text(step_label(label))
                .font(t::mono(Weight::Normal))
                .size(9.5)
                .color(t::MUTED),
        );
    }
    if app.busy && app.committing {
        items = items.push(
            text("Commit in progress — Cancel disabled")
                .font(t::mono(Weight::Normal))
                .size(9.5)
                .color(t::OXIDE_DARK),
        );
    } else if app.busy {
        items = items.push(
            button(text("Cancel").size(10).font(t::sans(Weight::Semibold)))
                .on_press(Message::Cancel)
                .padding([3, 9])
                .style(t::secondary),
        );
    }
    container(items)
        .padding([5, 14])
        .height(Length::Fixed(30.0))
        .into()
}

fn rail(app: &App) -> Element<'_, Message> {
    let remote_state = if app.remote.is_some() {
        "READY"
    } else {
        "OFFLINE"
    };
    let keys: usize = app
        .model
        .devices
        .iter()
        .map(|device| device.keys.len())
        .sum();
    let capacity = profile::skin_profile(&profile::HARMONY_300, app.model.skin)
        .map(|skin| skin.favourite_capacity)
        .unwrap_or(0);
    let items: Vec<Element<'_, Message>> = vec![
        nav_item(app, Page::Remote, "Remote", remote_state.to_string()),
        nav_item(
            app,
            Page::Devices,
            "My Devices",
            app.model.devices.len().to_string(),
        ),
        nav_item(app, Page::KeyMap, "Buttons", keys.to_string()),
        nav_item(
            app,
            Page::Activities,
            "Watch TV",
            app.model.activity.steps.len().to_string(),
        ),
        nav_item(
            app,
            Page::Favourites,
            "Favourites",
            format!("{}/{}", app.model.favourites.len(), capacity),
        ),
        nav_item(app, Page::Settings, "Settings", String::new()),
    ];
    let remote_line = match &app.remote {
        Some(remote) => format!(
            "{} · firmware {}",
            remote.model,
            remote.firmware_version.as_deref().unwrap_or("?")
        ),
        None => "Plug in the USB cable".to_string(),
    };
    container(
        column![
            text("Remote Rebirth")
                .font(t::serif())
                .size(21)
                .color(t::CREAM),
            text("HARMONY 300 · OWNER'S MANUAL")
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(t::COVER_TEXT),
            container(rule::horizontal(1.0).style(|_| rule::Style {
                color: t::COVER_LINE,
                radius: 0.0.into(),
                fill_mode: rule::FillMode::Full,
                snap: true,
            }))
            .padding([18, 0]),
            Column::with_children(items).spacing(2),
            iced::widget::Space::new().height(Fill),
            row![
                dot(if app.remote.is_some() {
                    t::GRASS
                } else {
                    t::FAINT
                }),
                text(if app.remote.is_some() {
                    "Remote connected"
                } else {
                    "No remote"
                })
                .font(t::sans(Weight::Medium))
                .size(10.5)
                .color(t::CREAM),
            ]
            .spacing(7)
            .align_y(Alignment::Center),
            text(remote_line)
                .font(t::mono(Weight::Normal))
                .size(8.5)
                .color(t::COVER_TEXT),
        ]
        .spacing(0),
    )
    .width(Length::Fixed(216.0))
    .height(Fill)
    .padding([22, 14])
    .style(t::rail_style)
    .into()
}

fn nav_item<'a>(
    app: &'a App,
    page: Page,
    label: &'static str,
    count: String,
) -> Element<'a, Message> {
    let active = app.page == page;
    let content = row![
        text(label)
            .font(t::sans(if active {
                Weight::Semibold
            } else {
                Weight::Normal
            }))
            .size(12.5),
        iced::widget::Space::new().width(Fill),
        text(count)
            .font(t::mono(Weight::Normal))
            .size(8)
            .color(if active { t::CREAM } else { t::COVER_TEXT }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let mut item = button(content).width(Fill).padding([7, 7]);
    if !app.busy {
        item = item.on_press(Message::Page(page));
    }
    item.style(if active { t::rail_active } else { t::rail })
        .into()
}

fn dot(color: Color) -> Element<'static, Message> {
    container(iced::widget::Space::new())
        .width(Length::Fixed(7.0))
        .height(Length::Fixed(7.0))
        .style(move |_| container::Style {
            background: Some(paint(color)),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn paint(color: Color) -> iced::Background {
    iced::Background::Color(color)
}

// -- remote -----------------------------------------------------------------

fn remote_page(app: &App) -> Element<'_, Message> {
    let current_backup: Element<'_, Message> = if app.advanced {
        match &app.backup_path {
            Some(path) => data_line("Current backup", &path.display().to_string()),
            None => iced::widget::Space::new().height(0).into(),
        }
    } else {
        iced::widget::Space::new().height(0).into()
    };
    let restore_field: Element<'_, Message> = if app.advanced {
        text_input("Path to backup file", &app.restore_path)
            .on_input(Message::RestorePath)
            .padding(6)
            .size(11)
            .font(t::mono(Weight::Normal))
            .style(t::text_field)
            .into()
    } else if app.restore_path.trim().is_empty() {
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
                    secondary("Find remote", Message::Detect, app.busy),
                    secondary("Save backup", Message::Backup, app.busy),
                    ghost("Refresh", Message::RefreshBackups, app.busy),
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
        if let Some(error) = &app.error {
            errata(app, error)
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
                        app.busy || app.validation().is_some()
                    ),
                ),
                procedure(
                    "2",
                    "Review the update",
                    "A backup is saved first. You see exactly what will be written.",
                    secondary(
                        "Review update",
                        Message::PrepareWrite,
                        app.busy || app.validation().is_some()
                    ),
                ),
                procedure(
                    "3",
                    "Write to the remote",
                    "The remote is erased, written, checked and read back.",
                    primary(
                        "Update remote",
                        Message::CommitWrite,
                        app.busy
                            || app.validation().is_some()
                            || app.prepared_kind != Some(PreparedKind::Write)
                    ),
                ),
                update_summary(app),
                checkbox(app.reboot)
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
                    secondary("Review restore", Message::PrepareRestore, app.busy),
                    primary(
                        "Restore remote",
                        Message::CommitRestore,
                        app.busy || app.prepared_kind != Some(PreparedKind::Restore)
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

fn identity_block(app: &App) -> Element<'_, Message> {
    match &app.remote {
        Some(remote) if app.advanced => column![
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

fn version_note(app: &App) -> Element<'_, Message> {
    if let Some(remote) = &app.remote
        && let Some(skin) = remote.skin
        && skin != app.model.skin
    {
        return column![
            note(if app.advanced {
                format!(
                    "The connected remote reports Skin {skin}; the setup uses Skin {}.",
                    app.model.skin
                )
            } else {
                "This setup was made for a different remote version.".into()
            }),
            secondary(
                "Use the connected remote's version",
                Message::UseDetectedSkin(skin),
                app.busy
            ),
        ]
        .spacing(6)
        .into();
    }
    iced::widget::Space::new().height(0).into()
}

fn backup_list(app: &App) -> Element<'_, Message> {
    if app.backups.is_empty() {
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
    for (index, path) in app.backups.iter().take(6).enumerate() {
        let (detail, validation) = backup_row(path, app.advanced);
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
                        app.busy || !verified
                    ),
                    danger("Remove", Message::DeleteBackup(path.clone()), app.busy),
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

fn backup_row(path: &std::path::Path, advanced: bool) -> (String, String) {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(".json");
    let Ok(bytes) = std::fs::read(std::path::PathBuf::from(sidecar)) else {
        return ("Saved backup".into(), "unknown".into());
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return ("Saved backup".into(), "unknown".into());
    };
    let validation = value["validation"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let detail = if advanced {
        format!(
            "{} · Skin {} · {} · {} B",
            value["model"].as_str().unwrap_or("unknown"),
            value["skin"]
                .as_u64()
                .map(|skin| skin.to_string())
                .unwrap_or_else(|| "?".into()),
            validation,
            value["size"].as_u64().unwrap_or(0)
        )
    } else if validation == "ok" {
        "Saved backup · valid".into()
    } else {
        "Saved backup · damaged".into()
    };
    (detail, validation)
}

fn backup_stamp(validation: &str) -> Element<'static, Message> {
    match validation {
        "ok" => stamp("Verified", t::GRASS),
        "checksum_mismatch" | "unreadable" => stamp("Invalid", t::OXIDE_DARK),
        _ => stamp("Unchecked", t::FAINT),
    }
}

fn update_summary(app: &App) -> Element<'_, Message> {
    let prepared = app
        .last_prepare
        .as_ref()
        .filter(|_| app.prepared_kind == Some(PreparedKind::Write))
        .map(|report| {
            if app.advanced {
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

fn phase_rail(app: &App) -> Element<'_, Message> {
    let current = match app.phase {
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

fn devices_page(app: &App) -> Element<'_, Message> {
    let left = column![
        section("Device slots", Some("FOUR DEVICE KEYS"), device_bays(app)),
        section(
            "Commands",
            Some(
                &app.selected_device()
                    .map(|device| format!("{} · {}", device.name, device.commands.len()))
                    .unwrap_or_else(|| "NO DEVICE".into())
            ),
            command_list(app),
        ),
    ]
    .spacing(18)
    .width(Fill);

    let right = column![
        section(
            "Device details",
            Some("NAME · DEVICE KEY"),
            device_details(app)
        ),
        section("Add a command", Some("IR SOURCE"), add_command_form(app)),
    ]
    .spacing(18)
    .width(Fill);

    row![
        scrollable(left).height(Fill).width(Fill),
        container(iced::widget::Space::new())
            .width(Length::Fixed(1.0))
            .height(Fill)
            .style(|_| container::Style {
                background: Some(paint(t::RULE)),
                ..Default::default()
            }),
        scrollable(right).height(Fill).width(Fill),
    ]
    .spacing(18)
    .height(Fill)
    .padding([14, 24])
    .into()
}

fn device_bays(app: &App) -> Element<'_, Message> {
    let mut bays: Vec<Element<'_, Message>> = Vec::new();
    for (slot, label) in profile::HARMONY_300_DEVICE_KEY_LABELS.iter().enumerate() {
        let slot_key = (slot + 1) as u8;
        let found = app
            .model
            .devices
            .iter()
            .enumerate()
            .find(|(_, device)| device.device_key == slot_key);
        let active = found
            .as_ref()
            .map(|(index, _)| *index == app.selected_device)
            .unwrap_or(false);
        let label_color = if active { t::OXIDE_DARK } else { t::MUTED };
        let content: Element<'_, Message> = match &found {
            Some((_, device)) => column![
                text(format!("DEVICE {}", slot + 1))
                    .font(t::mono(Weight::Normal))
                    .size(7.5)
                    .color(label_color),
                text(&device.name).font(t::serif()).size(14).color(t::INK),
                text(format!(
                    "{} command{}",
                    device.commands.len(),
                    if device.commands.len() == 1 { "" } else { "s" }
                ))
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(label_color),
            ]
            .spacing(3)
            .into(),
            None => column![
                text(format!("DEVICE {}", slot + 1))
                    .font(t::mono(Weight::Normal))
                    .size(7.5)
                    .color(t::FAINT),
                text(*label).font(t::serif()).size(14).color(t::FAINT),
                text("Empty slot")
                    .font(t::mono(Weight::Normal))
                    .size(8)
                    .color(t::FAINT),
            ]
            .spacing(3)
            .into(),
        };
        let mut cell = button(content)
            .width(Fill)
            .padding([8, 9])
            .style(t::bay(active));
        if let Some((index, _)) = found
            && !app.busy
        {
            cell = cell.on_press(Message::SelectDevice(index));
        }
        bays.push(cell.into());
    }
    column![
        Row::with_children(bays).spacing(6),
        row![
            text_input("Name, for example TV", &app.new_device_name)
                .on_input(Message::NewDeviceName)
                .padding(6)
                .size(11)
                .font(t::sans(Weight::Normal))
                .style(t::text_field)
                .width(Fill),
            pick_list(
                device_key_labels(),
                Some(device_key_label(app.new_device_key)),
                |label| Message::NewDeviceKey(device_key_from_label(&label))
            )
            .padding(6)
            .text_size(10)
            .font(t::sans(Weight::Normal))
            .style(t::select)
            .menu_style(t::select_menu),
            secondary("Add device", Message::AddDevice, app.busy),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn device_details(app: &App) -> Element<'_, Message> {
    let Some((index, device)) = app
        .model
        .devices
        .iter()
        .enumerate()
        .find(|(index, _)| *index == app.selected_device)
    else {
        return note("No device yet. Add one above.");
    };
    let name = match &app.device_name_draft {
        Some((draft_index, draft)) if *draft_index == index => draft.clone(),
        _ => device.name.clone(),
    };
    column![
        field(
            "Name",
            text_input("Device name", &name)
                .on_input(move |value| Message::DeviceName(index, value))
                .padding(6)
                .size(11)
                .font(t::sans(Weight::Normal))
                .style(t::text_field)
                .into()
        ),
        field(
            "Device key",
            pick_list(
                device_key_labels(),
                Some(device_key_label(device.device_key)),
                move |label| Message::DeviceKey(index, device_key_from_label(&label))
            )
            .padding(6)
            .text_size(10)
            .font(t::sans(Weight::Normal))
            .style(t::select)
            .menu_style(t::select_menu)
            .into()
        ),
        note(format!(
            "{} command{} on this device.",
            device.commands.len(),
            if device.commands.len() == 1 { "" } else { "s" }
        )),
        danger("Remove this device", Message::DeleteDevice(index), app.busy),
    ]
    .spacing(10)
    .into()
}

fn command_list(app: &App) -> Element<'_, Message> {
    let Some(device) = app.selected_device() else {
        return note("Choose a device first.");
    };
    if device.commands.is_empty() {
        return note("No commands yet. Add at least one below.");
    }
    let referenced = remote_rebirth_core::model::referenced_commands(&app.model);
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for (name, command) in &device.commands {
        let sent = referenced.contains(&(device.name.clone(), name.clone()));
        let source = format!(
            "{} {}",
            command.source.format,
            command
                .source
                .file
                .as_ref()
                .map(|path| path.display().to_string())
                .or_else(|| command.source.hex.as_ref().map(|_| "inline hex".into()))
                .unwrap_or_default()
        );
        let mut line = row![
            text(humanize(name))
                .font(t::sans(Weight::Medium))
                .size(11.5)
                .color(t::INK)
                .width(Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !sent {
            line = line.push(stamp("Not sent", t::OXIDE_DARK));
        }
        if app.advanced {
            line = line.push(
                text(source)
                    .font(t::mono(Weight::Normal))
                    .size(7.5)
                    .color(t::FAINT),
            );
        }
        line = line.push(danger(
            "Remove",
            Message::DeleteCommand(name.clone()),
            app.busy,
        ));
        rows.push(line.into());
        rows.push(rule::horizontal(1.0).style(t::rule_style).into());
    }
    column(rows).spacing(0).into()
}

fn add_command_form(app: &App) -> Element<'_, Message> {
    let form: &CommandForm = &app.new_command;
    column![
        field(
            "Name",
            text_input("Name, for example Power", &form.name)
                .on_input(Message::NewCommandName)
                .padding(6)
                .size(11)
                .font(t::sans(Weight::Normal))
                .style(t::text_field)
                .into()
        ),
        row![
            field(
                "Format",
                pick_list(
                    super::command_formats(),
                    Some(super::command_format_label(&form.format)),
                    |label| Message::NewCommandFormat(super::command_format_from_label(&label))
                )
                .padding(6)
                .text_size(10)
                .font(t::sans(Weight::Normal))
                .style(t::select)
                .menu_style(t::select_menu)
                .into()
            ),
            field(
                "Code file",
                text_input("Path to code file", &form.file)
                    .on_input(Message::NewCommandFile)
                    .padding(6)
                    .size(10)
                    .font(t::mono(Weight::Normal))
                    .style(t::text_field)
                    .into()
            ),
        ]
        .spacing(8),
        if form.format == "pronto" {
            field(
                "Pronto code",
                text_input("Paste Pronto code here", &form.hex)
                    .on_input(Message::NewCommandHex)
                    .padding(6)
                    .size(10)
                    .font(t::mono(Weight::Normal))
                    .style(t::text_field)
                    .into(),
            )
        } else if form.format == "lirc" {
            row![
                field(
                    "Remote",
                    text_input("Remote name", &form.remote)
                        .on_input(Message::NewCommandRemote)
                        .padding(6)
                        .size(10)
                        .font(t::mono(Weight::Normal))
                        .style(t::text_field)
                        .into()
                ),
                field(
                    "Button",
                    text_input("Button name", &form.button)
                        .on_input(Message::NewCommandButton)
                        .padding(6)
                        .size(10)
                        .font(t::mono(Weight::Normal))
                        .style(t::text_field)
                        .into()
                ),
            ]
            .spacing(8)
            .into()
        } else {
            field(
                "Command",
                text_input("Command name", &form.button)
                    .on_input(Message::NewCommandButton)
                    .padding(6)
                    .size(10)
                    .font(t::mono(Weight::Normal))
                    .style(t::text_field)
                    .into(),
            )
        },
        secondary("Add command", Message::AddCommand, app.busy),
    ]
    .spacing(8)
    .into()
}

// -- buttons ----------------------------------------------------------------

fn keymap_page(app: &App) -> Element<'_, Message> {
    let diagram = column![
        mono_label("FIG. 1 · KEY LAYOUT"),
        remote_diagram(app),
        note("Numbers match the list opposite."),
    ]
    .spacing(6)
    .width(Length::Fixed(318.0));

    let table = column![
        mono_label("FIG. 2 · ASSIGNED COMMANDS"),
        selected_key_summary(app),
        container(key_table(app)).height(Length::FillPortion(3)),
        container(command_index(app)).height(Length::FillPortion(2)),
    ]
    .spacing(8)
    .height(Fill)
    .width(Fill);

    column![
        device_tabs(app),
        routing_band(app),
        row![
            scrollable(diagram).height(Fill).width(Length::Fixed(318.0)),
            container(iced::widget::Space::new())
                .width(Length::Fixed(1.0))
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(paint(t::RULE)),
                    ..Default::default()
                }),
            table,
        ]
        .spacing(14)
        .height(Fill),
    ]
    .spacing(10)
    .height(Fill)
    .padding([12, 24])
    .into()
}

fn device_tabs(app: &App) -> Element<'_, Message> {
    let names = app.device_names();
    let mut tabs: Vec<Element<'_, Message>> = Vec::new();
    for (slot, label) in profile::HARMONY_300_DEVICE_KEY_LABELS.iter().enumerate() {
        let slot_key = (slot + 1) as u8;
        let found = app
            .model
            .devices
            .iter()
            .enumerate()
            .find(|(_, device)| device.device_key == slot_key);
        let active = found
            .as_ref()
            .map(|(index, _)| *index == app.selected_device)
            .unwrap_or(false);
        let (name, count) = match &found {
            Some((_, device)) => (device.name.clone(), format!("{} keys", device.keys.len())),
            None => (format!("Empty {label}"), "—".into()),
        };
        let content = row![
            text(format!("{} ", slot + 1))
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(if active { t::OXIDE_DARK } else { t::FAINT }),
            text(name).font(t::sans(Weight::Semibold)).size(10.5),
            text(count)
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED),
        ]
        .spacing(5)
        .align_y(Alignment::Center);
        let mut tab = button(content)
            .padding([6, 10])
            .height(Length::Fixed(32.0))
            .style(t::tab(active));
        if let Some((index, _)) = found
            && !app.busy
        {
            tab = tab.on_press(Message::SelectDevice(index));
        }
        tabs.push(tab.into());
    }
    let hint = if names.is_empty() {
        "Add a device on My Devices first.".to_string()
    } else {
        format!(
            "{} device{}",
            names.len(),
            if names.len() == 1 { "" } else { "s" }
        )
    };
    column![
        Row::with_children(tabs).spacing(0).align_y(Alignment::End),
        rule::horizontal(1.0).style(t::ink_rule),
        text(hint)
            .font(t::mono(Weight::Normal))
            .size(8)
            .color(t::FAINT),
    ]
    .spacing(2)
    .into()
}

fn routing_band(app: &App) -> Element<'_, Message> {
    let names = app.device_names();
    let mut options = vec!["Each device itself".to_string()];
    options.extend(names);
    let volume = app
        .model
        .routing
        .as_ref()
        .and_then(|routing| routing.volume.clone())
        .unwrap_or_else(|| "Each device itself".into());
    let channel = app
        .model
        .routing
        .as_ref()
        .and_then(|routing| routing.channel.clone())
        .unwrap_or_else(|| "Each device itself".into());
    let volume_keys = if volume == "Each device itself" {
        "Every device keeps its own keys.".to_string()
    } else {
        "Vol + · Vol − · Mute".to_string()
    };
    let channel_keys = if channel == "Each device itself" {
        "Every device keeps its own keys.".to_string()
    } else {
        "Ch + · Ch −".to_string()
    };
    container(
        row![
            route_control(
                "VOLUME",
                pick_list(options.clone(), Some(volume), |value| {
                    Message::RoutingVolume(none_if_placeholder(value))
                }),
                volume_keys
            ),
            route_control(
                "CHANNEL",
                pick_list(options, Some(channel), |value| Message::RoutingChannel(
                    none_if_placeholder(value)
                )),
                channel_keys
            ),
        ]
        .spacing(0),
    )
    .padding([7, 10])
    .style(t::plate)
    .into()
}

fn route_control<'a>(
    label: &'a str,
    select: impl Into<Element<'a, Message>>,
    keys: String,
) -> Element<'a, Message> {
    column![
        row![
            text(label)
                .font(t::mono(Weight::Medium))
                .size(8)
                .color(t::MUTED)
                .width(Length::Fixed(64.0)),
            select.into(),
            text("→")
                .font(t::mono(Weight::Normal))
                .size(12)
                .color(t::OXIDE),
            text(keys)
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(t::MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .width(Fill)
    .padding([0, 10])
    .into()
}

fn remote_diagram(app: &App) -> Element<'_, Message> {
    let selected = app.selected_key.as_deref();
    let remote = app
        .remote
        .as_ref()
        .map(|remote| remote.model.clone())
        .unwrap_or_else(|| "Harmony 300".into());
    let device_name = app
        .selected_device()
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "No device".into());
    let device_keys: Vec<Element<'_, Message>> = profile::HARMONY_300_DEVICE_KEY_LABELS
        .iter()
        .enumerate()
        .map(|(slot, label)| {
            let slot_key = (slot + 1) as u8;
            let found = app
                .model
                .devices
                .iter()
                .enumerate()
                .find(|(_, device)| device.device_key == slot_key);
            let active = found
                .as_ref()
                .map(|(index, _)| *index == app.selected_device)
                .unwrap_or(false);
            let caption = match &found {
                Some((_, device)) => device.name.clone(),
                None => label.to_string(),
            };
            let mut key = button(
                column![
                    text(format!("{}", slot + 1))
                        .font(t::mono(Weight::Normal))
                        .size(7)
                        .color(if active { t::CREAM } else { t::MUTED }),
                    text(caption)
                        .font(t::sans(Weight::Medium))
                        .size(6.5)
                        .color(if active { t::CREAM } else { t::INK }),
                ]
                .spacing(1)
                .align_x(Alignment::Center),
            )
            .width(Fill)
            .height(Length::Fixed(24.0))
            .padding(0)
            .style(t::key(active, false, found.is_none()));
            if let Some((index, _)) = found
                && !app.busy
            {
                key = key.on_press(Message::SelectDevice(index));
            }
            key.into()
        })
        .collect();

    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for (chunk_index, chunk) in profile::HARMONY_300
        .key_layout
        .chunks(KEY_COLUMNS)
        .enumerate()
    {
        let mut cells: Vec<Element<'_, Message>> = Vec::new();
        for (offset, binding) in chunk.iter().enumerate() {
            let index = chunk_index * KEY_COLUMNS + offset + 1;
            let is_selected = selected == Some(binding.name);
            let inherited = routed_target(&app.model, binding.name, &device_name);
            let empty = app
                .selected_device()
                .map(|device| !device.keys.contains_key(binding.name))
                .unwrap_or(true);
            let content = row![
                text(format!("{index:02}"))
                    .font(t::mono(Weight::Normal))
                    .size(6.5)
                    .color(if is_selected { t::CREAM } else { t::OXIDE }),
                text(key_short(binding.name))
                    .font(t::sans(Weight::Medium))
                    .size(7.2)
                    .color(if is_selected { t::CREAM } else { t::INK }),
            ]
            .spacing(4)
            .align_y(Alignment::Center);
            let mut key = button(content)
                .width(Fill)
                .height(Length::Fixed(20.0))
                .padding([0, 4])
                .style(t::key(is_selected, inherited.is_some(), empty));
            if !app.busy && inherited.is_none() {
                key = key.on_press(Message::SelectKey(binding.name.into()));
            }
            cells.push(key.into());
        }
        rows.push(Row::with_children(cells).spacing(1).into());
    }

    container(
        column![
            container(
                column![
                    text(device_name).font(t::serif()).size(11).color(t::CREAM),
                    text(
                        selected
                            .and_then(|key| app
                                .selected_device()
                                .and_then(|device| device.keys.get(key)))
                            .map(|command| format!("Sends {}", humanize(command)))
                            .unwrap_or_else(|| format!("{} · ready", remote))
                    )
                    .font(t::mono(Weight::Normal))
                    .size(7)
                    .color(t::RULE),
                ]
                .spacing(2),
            )
            .width(Fill)
            .padding([5, 7])
            .style(|_| container::Style {
                background: Some(paint(t::INK)),
                ..Default::default()
            }),
            Row::with_children(device_keys).spacing(2),
            Column::with_children(rows).spacing(1),
        ]
        .spacing(4),
    )
    .width(Length::Fixed(288.0))
    .padding(6)
    .style(|_| container::Style {
        border: iced::Border {
            width: 1.0,
            color: t::INK,
            radius: 18.0.into(),
        },
        background: Some(paint(t::PAPER_2)),
        ..Default::default()
    })
    .into()
}

fn key_short(name: &str) -> &'static str {
    match name {
        "power" => "PWR",
        "volume_up" => "VOL+",
        "volume_down" => "VOL−",
        "volume_mute" => "MUTE",
        "channel_up" => "CH+",
        "channel_down" => "CH−",
        "number_0" => "0",
        "number_1" => "1",
        "number_2" => "2",
        "number_3" => "3",
        "number_4" => "4",
        "number_5" => "5",
        "number_6" => "6",
        "number_7" => "7",
        "number_8" => "8",
        "number_9" => "9",
        "direction_up" => "UP",
        "direction_down" => "DN",
        "direction_left" => "LT",
        "direction_right" => "RT",
        "select" => "OK",
        "menu" => "MENU",
        "exit" => "EXIT",
        "info" => "INFO",
        "guide" => "GUIDE",
        "play" => "PLAY",
        "pause" => "PAUSE",
        "stop" => "STOP",
        "rewind" => "REW",
        "fast_forward" => "FF",
        "skip_back" => "|<<",
        "skip_forward" => ">>|",
        "record" => "REC",
        "red" => "RED",
        "green" => "GRN",
        "yellow" => "YEL",
        "blue" => "BLU",
        "list" => "LIST",
        "prev_channel" => "PREV",
        "clear" => "CLR",
        "enter" => "ENT",
        "input" => "INPUT",
        "teletext" => "TXT",
        _ => "—",
    }
}

fn selected_key_summary(app: &App) -> Element<'_, Message> {
    let Some(key) = &app.selected_key else {
        return note("Choose a key in the drawing.");
    };
    let Some(device) = app.selected_device() else {
        return note("Add a device first.");
    };
    let index = profile::HARMONY_300
        .key_layout
        .iter()
        .position(|binding| binding.name == key.as_str())
        .map(|index| index + 1)
        .unwrap_or(0);
    let inherited = routed_target(&app.model, key, &device.name);
    let sends = if let Some(target) = &inherited {
        format!("Inherited from {target}")
    } else {
        device
            .keys
            .get(key)
            .map(|command| format!("Sends {}", humanize(command)))
            .unwrap_or_else(|| "Nothing assigned".into())
    };
    row![
        text(format!("{index:02}"))
            .font(t::mono(Weight::Medium))
            .size(10)
            .color(t::OXIDE),
        text(humanize(key)).font(t::serif()).size(17).color(t::INK),
        text(sends)
            .font(t::sans(Weight::Normal))
            .size(10.5)
            .color(t::MUTED),
        iced::widget::Space::new().width(Fill),
        ghost("Clear", Message::ClearKey, app.busy || inherited.is_some()),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn key_table(app: &App) -> Element<'_, Message> {
    let Some(device) = app.selected_device() else {
        return note("No device selected.");
    };
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for (index, binding) in profile::HARMONY_300.key_layout.iter().enumerate() {
        let inherited = routed_target(&app.model, binding.name, &device.name);
        let (command, target, state, state_color) = if let Some(target) = &inherited {
            let command = app
                .model
                .devices
                .iter()
                .find(|candidate| &candidate.name == target)
                .and_then(|candidate| candidate.keys.get(binding.name))
                .map(|command| humanize(command))
                .unwrap_or_else(|| format!("Missing in {target}"));
            (command, target.clone(), "Inherited".to_string(), t::COBALT)
        } else if let Some(command) = device.keys.get(binding.name) {
            (
                humanize(command),
                device.name.clone(),
                "Assigned".to_string(),
                t::GRASS,
            )
        } else {
            (
                "Not assigned".to_string(),
                "—".to_string(),
                "Empty".to_string(),
                t::OXIDE_DARK,
            )
        };
        let selected = app.selected_key.as_deref() == Some(binding.name);
        let content = row![
            text(format!("{:02}", index + 1))
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(if selected { t::OXIDE_DARK } else { t::FAINT })
                .width(Length::Fixed(24.0)),
            text(humanize(binding.name))
                .font(t::sans(Weight::Medium))
                .size(10)
                .color(t::INK)
                .width(Length::Fixed(84.0)),
            text(command)
                .font(t::sans(Weight::Normal))
                .size(10)
                .color(t::INK)
                .width(Fill),
            text(target)
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED)
                .width(Length::Fixed(82.0)),
            text(state)
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(state_color)
                .width(Length::Fixed(62.0)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let mut line = button(content)
            .width(Fill)
            .padding([4, 4])
            .style(t::row_button(selected));
        if !app.busy && inherited.is_none() {
            line = line.on_press(Message::SelectKey(binding.name.into()));
        }
        rows.push(line.into());
        if index + 1 < profile::HARMONY_300.key_layout.len() {
            rows.push(rule::horizontal(1.0).style(t::rule_style).into());
        }
    }

    column![
        row![
            text("№")
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED)
                .width(Length::Fixed(24.0)),
            text("BUTTON")
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED)
                .width(Length::Fixed(84.0)),
            text("SENDS")
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED)
                .width(Fill),
            text("DEVICE")
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED)
                .width(Length::Fixed(82.0)),
            text("STATE")
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED)
                .width(Length::Fixed(62.0)),
        ]
        .spacing(6),
        rule::horizontal(1.0).style(t::ink_rule),
        scrollable(Column::with_children(rows))
            .height(Fill)
            .style(t::scrollable),
    ]
    .spacing(2)
    .height(Fill)
    .into()
}

fn command_index(app: &App) -> Element<'_, Message> {
    let Some(device) = app.selected_device() else {
        return note("No device selected.");
    };
    let filter = app.command_filter.to_lowercase();
    let referenced = remote_rebirth_core::model::referenced_commands(&app.model);
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for (name, command) in &device.commands {
        if !filter.is_empty() && !name.to_lowercase().contains(&filter) {
            continue;
        }
        let used = referenced.contains(&(device.name.clone(), name.clone()));
        let mut content = row![
            text(humanize(name))
                .font(t::sans(Weight::Medium))
                .size(10)
                .color(t::INK),
            text(if used { "used" } else { "not sent" })
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(if used { t::GRASS } else { t::FAINT }),
            iced::widget::Space::new().width(Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if app.advanced {
            content = content.push(
                text(&command.source.format)
                    .font(t::mono(Weight::Normal))
                    .size(7.5)
                    .color(t::MUTED),
            );
        }
        let mut item = button(content)
            .width(Fill)
            .padding([3, 5])
            .style(t::row_button(false));
        if app.selected_key.is_some() && !app.busy {
            item = item.on_press(Message::AssignCommand(name.clone()));
        }
        rows.push(item.into());
        rows.push(rule::horizontal(1.0).style(t::rule_style).into());
    }
    if rows.is_empty() {
        rows.push(note("No matching commands."));
    }
    let unused = device
        .commands
        .keys()
        .filter(|name| !referenced.contains(&(device.name.clone(), name.to_string())))
        .count();
    column![
        rule::horizontal(1.0).style(t::ink_rule),
        row![
            text("Command index")
                .font(t::serif())
                .size(15)
                .color(t::INK),
            iced::widget::Space::new().width(Fill),
            text(format!(
                "{} COMMANDS · {unused} NOT SENT",
                device.commands.len()
            ))
            .font(t::mono(Weight::Normal))
            .size(7.5)
            .color(t::FAINT),
        ]
        .align_y(Alignment::Center),
        text_input("Search the index", &app.command_filter)
            .on_input(Message::CommandFilter)
            .padding(5)
            .size(10)
            .font(t::sans(Weight::Normal))
            .style(t::text_field),
        scrollable(Column::with_children(rows))
            .height(Fill)
            .style(t::scrollable),
    ]
    .spacing(5)
    .height(Fill)
    .into()
}

// -- activities, favourites, settings ---------------------------------------

fn activities_page(app: &App) -> Element<'_, Message> {
    let names = app.device_names();
    let mut steps: Vec<Element<'_, Message>> = Vec::new();
    for (index, step) in app.model.activity.steps.iter().enumerate() {
        steps.push(
            row![
                text(format!("{:02}", index + 1))
                    .font(t::serif())
                    .size(17)
                    .color(t::OXIDE)
                    .width(Length::Fixed(30.0)),
                pick_list(names.clone(), Some(step.device.clone()), move |device| {
                    Message::StepDevice(index, device)
                })
                .padding(5)
                .text_size(10)
                .font(t::sans(Weight::Normal))
                .style(t::select)
                .menu_style(t::select_menu)
                .width(Length::Fixed(170.0)),
                field(
                    "Power",
                    pick_list(
                        command_options(app, &step.device, step.power.as_deref()),
                        Some(command_selected(step.power.as_deref())),
                        move |value| Message::StepPower(index, command_choice(value))
                    )
                    .padding(5)
                    .text_size(10)
                    .font(t::sans(Weight::Normal))
                    .style(t::select)
                    .menu_style(t::select_menu)
                    .width(Length::Fixed(130.0))
                    .into()
                ),
                field(
                    "Input",
                    pick_list(
                        command_options(app, &step.device, step.input.as_deref()),
                        Some(command_selected(step.input.as_deref())),
                        move |value| Message::StepInput(index, command_choice(value))
                    )
                    .padding(5)
                    .text_size(10)
                    .font(t::sans(Weight::Normal))
                    .style(t::select)
                    .menu_style(t::select_menu)
                    .width(Length::Fixed(130.0))
                    .into()
                ),
                field(
                    "Wait ms",
                    text_input("ms", &step.delay_ms.to_string())
                        .on_input(move |value| Message::StepDelay(index, value))
                        .padding(5)
                        .size(10)
                        .font(t::mono(Weight::Normal))
                        .style(t::text_field)
                        .into()
                ),
                danger("Delete", Message::DeleteStep(index), app.busy),
            ]
            .spacing(8)
            .align_y(Alignment::End)
            .into(),
        );
    }
    let step_list: Element<'_, Message> = if steps.is_empty() {
        note("No devices in the activity yet.")
    } else {
        Column::with_children(steps).spacing(4).into()
    };
    let add_power = command_options(app, &app.activity_device, Some(&app.activity_power));
    let add_input = command_options(app, &app.activity_device, Some(&app.activity_input));

    scrollable(
        column![
            section(
                "The activity",
                Some("ACTIVITY"),
                column![
                    body("One button switches the devices on in order. Choose the device that changes channels."),
                    field(
                        "Changes channels",
                        pick_list(
                            names.clone(),
                            Some(app.model.activity.channel_device.clone()),
                            Message::ActivityChannel
                        )
                        .padding(6)
                        .text_size(10)
                        .font(t::sans(Weight::Normal))
                        .style(t::select)
                        .menu_style(t::select_menu)
                        .into()
                    ),
                ]
                .spacing(8)
                .into(),
            ),
            section("Steps", Some("ORDER"), step_list),
            section(
                "Add a step",
                Some("DEVICE · COMMANDS"),
                column![
                    field(
                        "Device",
                        pick_list(
                            names,
                            Some(app.activity_device.clone()),
                            Message::ActivityDevice
                        )
                        .padding(6)
                        .text_size(10)
                        .font(t::sans(Weight::Normal))
                        .style(t::select)
                        .menu_style(t::select_menu)
                        .into()
                    ),
                    row![
                        field(
                            "Power command",
                            pick_list(
                                add_power,
                                Some(command_selected(Some(&app.activity_power))),
                                |value| Message::ActivityPower(command_choice(value))
                            )
                            .padding(6)
                            .text_size(10)
                            .font(t::sans(Weight::Normal))
                            .style(t::select)
                            .menu_style(t::select_menu)
                            .into()
                        ),
                        field(
                            "Input command",
                            pick_list(
                                add_input,
                                Some(command_selected(Some(&app.activity_input))),
                                |value| Message::ActivityInput(command_choice(value))
                            )
                            .padding(6)
                            .text_size(10)
                            .font(t::sans(Weight::Normal))
                            .style(t::select)
                            .menu_style(t::select_menu)
                            .into()
                        ),
                        field(
                            "Wait (ms)",
                            text_input("Wait (ms)", &app.activity_delay)
                                .on_input(Message::ActivityDelay)
                                .padding(6)
                                .size(10)
                                .font(t::mono(Weight::Normal))
                                .style(t::text_field)
                                .into()
                        ),
                    ]
                    .spacing(8),
                    secondary("Add step", Message::AddStep, app.busy),
                ]
                .spacing(8)
                .into(),
            ),
        ]
        .spacing(18)
        .padding([14, 24]),
    )
    .height(Fill)
    .style(t::scrollable)
    .into()
}

fn favourites_page(app: &App) -> Element<'_, Message> {
    let capacity = profile::skin_profile(&profile::HARMONY_300, app.model.skin)
        .map(|skin| skin.favourite_capacity)
        .unwrap_or(0);
    let mut slots: Vec<Element<'_, Message>> = Vec::new();
    for index in 0..capacity {
        match app.model.favourites.get(index) {
            Some(favourite) => {
                slots.push(
                    row![
                        text(format!("{}", index + 1))
                            .font(t::serif())
                            .size(24)
                            .color(t::OXIDE)
                            .width(Length::Fixed(36.0)),
                        text(format!("Button {}", favourite.key))
                            .font(t::mono(Weight::Normal))
                            .size(8.5)
                            .color(t::MUTED)
                            .width(Length::Fixed(78.0)),
                        text_input("Channel", &favourite.channel)
                            .on_input(move |value| Message::FavouriteChannel(index, value))
                            .padding(6)
                            .size(12)
                            .font(t::sans(Weight::Semibold))
                            .style(t::text_field)
                            .width(Length::Fixed(180.0)),
                        iced::widget::Space::new().width(Fill),
                        danger("Remove", Message::DeleteFavourite(index), app.busy),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                );
            }
            None => slots.push(
                row![
                    text(format!("{}", index + 1))
                        .font(t::serif())
                        .size(24)
                        .color(t::RULE)
                        .width(Length::Fixed(36.0)),
                    text("Empty slot")
                        .font(t::mono(Weight::Normal))
                        .size(9)
                        .color(t::FAINT),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
            ),
        }
    }
    let slots_element: Element<'_, Message> = if capacity == 0 {
        note("This remote has no favourite buttons.")
    } else {
        Column::with_children(slots).spacing(6).into()
    };

    scrollable(
        column![
            section(
                "Channels",
                Some(&format!("SKIN {} · {} SLOTS", app.model.skin, capacity)),
                column![
                    body(format!(
                        "Your remote has {capacity} favourite channel button{}. The numbers are printed on the remote.",
                        if capacity == 1 { "" } else { "s" }
                    )),
                    slots_element,
                ]
                .spacing(10)
                .into(),
            ),
            section(
                "Add a favourite",
                Some("CHANNEL NUMBER"),
                row![
                    text_input("Channel number", &app.new_favourite_channel)
                        .on_input(Message::NewFavouriteChannel)
                        .padding(6)
                        .size(11)
                        .font(t::sans(Weight::Normal))
                        .style(t::text_field)
                        .width(Length::Fixed(200.0)),
                    primary("Add", Message::AddFavourite, app.busy),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ),
        ]
        .spacing(18)
        .padding([14, 24]),
    )
    .height(Fill)
    .style(t::scrollable)
    .into()
}

fn settings_page(app: &App) -> Element<'_, Message> {
    let display = section(
        "Display",
        Some("PREFERENCES"),
        column![
            body("Changes are saved automatically."),
            checkbox(app.advanced)
                .label("Show technical details")
                .text_size(11.5)
                .font(t::sans(Weight::Normal))
                .on_toggle(Message::ToggleAdvanced)
                .style(t::checkbox),
        ]
        .spacing(8)
        .into(),
    );
    let about = section(
        "About",
        Some("COLOPHON"),
        column![
            text("Remote Rebirth")
                .font(t::serif())
                .size(18)
                .color(t::INK),
            body("Offline setup for your Harmony 300. Works without an internet connection."),
            body("GPL-3.0-or-later. No account, no telemetry, no update check."),
            data_line("Version", env!("CARGO_PKG_VERSION")),
        ]
        .spacing(6)
        .into(),
    );
    let left = column![display, about].spacing(18).width(Fill);

    let right: Element<'_, Message> = if app.advanced {
        column![
            section(
                "Remote setup",
                Some("FILES"),
                column![
                    data_line("Data directory", &app.data_dir.display().to_string()),
                    data_line("Model", &app.model_path.display().to_string()),
                    data_line("Container", &app.container_path.display().to_string()),
                    data_line("Backups", &app.backups_dir.display().to_string()),
                    row![
                        text("Skin")
                            .font(t::mono(Weight::Normal))
                            .size(9)
                            .color(t::MUTED)
                            .width(Length::Fixed(90.0)),
                        pick_list(vec![78u8, 79], Some(app.model.skin), Message::SkinChanged)
                            .padding(5)
                            .text_size(10)
                            .font(t::sans(Weight::Normal))
                            .style(t::select)
                            .menu_style(t::select_menu),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        secondary("Save now", Message::Save, app.busy),
                        secondary("Reload", Message::Reload, app.busy),
                    ]
                    .spacing(6),
                ]
                .spacing(8)
                .into(),
            ),
            section(
                "Configuration file",
                Some("MODEL JSON"),
                column![
                    body("The active setup is ordinary Model JSON. Export it for the CLI, or import a setup prepared elsewhere."),
                    text_input("Path to Model JSON", &app.model_transfer_path)
                        .on_input(Message::TransferPath)
                        .padding(6)
                        .size(10)
                        .font(t::mono(Weight::Normal))
                        .style(t::text_field),
                    row![
                        secondary("Export Model", Message::ExportModel, app.busy),
                        secondary("Import Model", Message::ImportModel, app.busy),
                    ]
                    .spacing(6),
                ]
                .spacing(8)
                .into(),
            ),
        ]
        .spacing(18)
        .into()
    } else {
        column![
            section(
                "Technical details",
                Some("HIDDEN"),
                column![
                    note("File paths, the portable Model, and the remote's raw identity are available behind Show technical details."),
                ]
                .spacing(8)
                .into(),
            ),
        ]
        .spacing(18)
        .into()
    };

    scrollable(row![left, right].spacing(28).padding([14, 24]).height(Fill))
        .height(Fill)
        .style(t::scrollable)
        .into()
}

// -- small parts -------------------------------------------------------------

fn command_options(app: &App, device_name: &str, current: Option<&str>) -> Vec<String> {
    let mut options = vec![NO_COMMAND.to_string()];
    if let Some(device) = app
        .model
        .devices
        .iter()
        .find(|device| device.name == device_name)
    {
        options.extend(device.commands.keys().cloned());
    }
    if let Some(current) = current
        && !current.is_empty()
        && !options.iter().any(|option| option == current)
    {
        options.push(current.to_string());
    }
    options
}

fn command_selected(current: Option<&str>) -> String {
    current
        .filter(|value| !value.is_empty())
        .unwrap_or(NO_COMMAND)
        .to_string()
}

fn command_choice(value: String) -> String {
    if value == NO_COMMAND {
        String::new()
    } else {
        value
    }
}

fn section<'a>(
    title: &'static str,
    note: Option<&str>,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut head =
        row![text(title).font(t::serif()).size(18).color(t::INK)].align_y(Alignment::Center);
    if let Some(note) = note {
        head = head.push(iced::widget::Space::new().width(Fill));
        head = head.push(
            text(note.to_string())
                .font(t::mono(Weight::Normal))
                .size(8)
                .color(t::FAINT),
        );
    }
    column![head, content, rule::horizontal(1.0).style(t::rule_style)]
        .spacing(8)
        .into()
}

fn procedure<'a>(
    number: &'a str,
    title: &'a str,
    detail: &'a str,
    action: Element<'a, Message>,
) -> Element<'a, Message> {
    row![
        text(number)
            .font(t::serif())
            .size(22)
            .color(t::OXIDE)
            .width(Length::Fixed(28.0)),
        column![
            text(title)
                .font(t::sans(Weight::Semibold))
                .size(11.5)
                .color(t::INK),
            text(detail)
                .font(t::sans(Weight::Normal))
                .size(10)
                .color(t::MUTED),
            action,
        ]
        .spacing(4)
        .width(Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Start)
    .into()
}

fn field<'a>(label: &'a str, control: Element<'a, Message>) -> Element<'a, Message> {
    column![
        text(label.to_uppercase())
            .font(t::mono(Weight::Normal))
            .size(7.5)
            .color(t::MUTED),
        control,
    ]
    .spacing(3)
    .width(Fill)
    .into()
}

fn data_line(label: &str, value: &str) -> Element<'static, Message> {
    row![
        text(label.to_string())
            .font(t::mono(Weight::Normal))
            .size(8.5)
            .color(t::MUTED)
            .width(Length::Fixed(96.0)),
        text(value.to_string())
            .font(t::mono(Weight::Normal))
            .size(9)
            .color(t::INK),
    ]
    .spacing(6)
    .into()
}

fn mono_label(value: &str) -> Element<'static, Message> {
    text(value.to_uppercase())
        .font(t::mono(Weight::Medium))
        .size(8)
        .color(t::MUTED)
        .into()
}

fn body(value: impl ToString) -> Element<'static, Message> {
    text(value.to_string())
        .font(t::sans(Weight::Normal))
        .size(11.5)
        .color(t::INK)
        .into()
}

fn note(value: impl ToString) -> Element<'static, Message> {
    text(value.to_string())
        .font(t::sans(Weight::Normal))
        .size(10.5)
        .color(t::MUTED)
        .into()
}

fn stamp(label: &str, color: Color) -> Element<'static, Message> {
    container(
        text(label.to_uppercase())
            .font(t::mono(Weight::Medium))
            .size(7.5)
            .color(color),
    )
    .padding([2, 5])
    .style(t::stamp(color))
    .into()
}

fn errata(app: &App, error: &str) -> Element<'static, Message> {
    let mut detail = column![
        text("ERRATA")
            .font(t::mono(Weight::Medium))
            .size(8)
            .color(t::OXIDE_DARK),
        text(friendly_error(error))
            .font(t::sans(Weight::Normal))
            .size(11)
            .color(t::INK),
    ]
    .spacing(3);
    if app.advanced {
        detail = detail.push(
            text(error.to_string())
                .font(t::mono(Weight::Normal))
                .size(8.5)
                .color(t::MUTED),
        );
        detail = detail.push(data_line(
            "Activity log",
            &remote_rebirth_core::state_dir()
                .join("journal.jsonl")
                .display()
                .to_string(),
        ));
    }
    let mut actions = row![
        ghost(
            if app.advanced {
                "Hide details"
            } else {
                "Technical details"
            },
            Message::ToggleAdvanced(!app.advanced),
            false
        ),
        ghost("Dismiss", Message::DismissError, false),
    ]
    .spacing(4);
    if let Some(path) = &app.backup_path {
        actions = actions.push(ghost(
            "Restore this backup",
            Message::UseBackup(path.clone()),
            app.busy || backup_row(path, false).1 != "ok",
        ));
    }
    container(
        row![detail.width(Fill), actions,]
            .spacing(10)
            .align_y(Alignment::Start),
    )
    .padding([8, 10])
    .style(t::errata)
    .into()
}

fn friendly_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("no harmony remote found") || lower.contains("no remote found") {
        "No Harmony remote found. Connect it with the USB cable and try again.".into()
    } else if lower.contains("multiple harmony remotes") {
        "More than one Harmony remote is connected. Disconnect all but one.".into()
    } else if lower.contains("declares 0 devices") {
        "Add at least one device before checking the setup.".into()
    } else if lower.contains("does not bind key") {
        "Choose the volume or channel buttons for the target device first.".into()
    } else if lower.contains("which routing") {
        "This button is controlled by another device. Change the volume or channel control first."
            .into()
    } else if lower.contains("backup validation") || lower.contains("unverified backup") {
        "That backup is damaged or incomplete. Choose another backup.".into()
    } else if lower.contains("current configuration does not match backup") {
        "The remote has changed since the backup. Save a new backup, then try again.".into()
    } else if lower.contains("read-back mismatch") {
        "The remote did not store the update correctly. Restore your backup.".into()
    } else if lower.contains("container is for skin") {
        "This setup is for a different remote version. Use the connected remote's version, then check the setup again.".into()
    } else if lower.contains("container is") || lower.contains("container checksum") {
        "The new setup is not valid. Check the devices and buttons.".into()
    } else if lower.contains("link_packet_length") {
        "This remote is not responding correctly. Reconnect it and try again.".into()
    } else {
        error.to_string()
    }
}

fn primary(label: &'static str, message: Message, disabled: bool) -> Element<'static, Message> {
    button(text(label).size(11).font(t::sans(Weight::Semibold)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([6, 12])
        .style(t::primary)
        .into()
}

fn secondary(label: &'static str, message: Message, disabled: bool) -> Element<'static, Message> {
    button(text(label).size(10.5).font(t::sans(Weight::Medium)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([6, 10])
        .style(t::secondary)
        .into()
}

fn ghost(label: &'static str, message: Message, disabled: bool) -> Element<'static, Message> {
    button(text(label).size(10).font(t::sans(Weight::Medium)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([4, 6])
        .style(t::ghost)
        .into()
}

fn danger(label: &'static str, message: Message, disabled: bool) -> Element<'static, Message> {
    button(text(label).size(10).font(t::sans(Weight::Medium)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([4, 6])
        .style(t::danger)
        .into()
}
