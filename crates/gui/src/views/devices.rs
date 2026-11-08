//! The My Devices page: device slots, details, and commands.

use super::widgets::{danger, field, note, paint, secondary, section, stamp};
use crate::app::App;
use crate::message::{CommandForm, Message};
use crate::support::{device_key_from_label, device_key_label, device_key_labels, humanize};
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{
    Row, button, column, container, pick_list, row, rule, scrollable, text, text_input,
};
use iced::{Alignment, Element, Fill, Length};
use remote_rebirth_core::profile;

pub(super) fn devices_page(app: &App) -> Element<'_, Message> {
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

pub(super) fn device_bays(app: &App) -> Element<'_, Message> {
    let mut bays: Vec<Element<'_, Message>> = Vec::new();
    for (slot, label) in profile::HARMONY_300_DEVICE_KEY_LABELS.iter().enumerate() {
        let slot_key = (slot + 1) as u8;
        let found = app
            .project
            .model
            .devices
            .iter()
            .enumerate()
            .find(|(_, device)| device.device_key == slot_key);
        let active = found
            .as_ref()
            .map(|(index, _)| *index == app.ui.selected_device)
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
            && !app.operation.busy
        {
            cell = cell.on_press(Message::SelectDevice(index));
        }
        bays.push(cell.into());
    }
    column![
        Row::with_children(bays).spacing(6),
        row![
            text_input("Name, for example TV", &app.ui.new_device_name)
                .on_input(Message::NewDeviceName)
                .padding(6)
                .size(11)
                .font(t::sans(Weight::Normal))
                .style(t::text_field)
                .width(Fill),
            pick_list(
                device_key_labels(),
                Some(device_key_label(app.ui.new_device_key)),
                |label| Message::NewDeviceKey(device_key_from_label(&label))
            )
            .padding(6)
            .text_size(10)
            .font(t::sans(Weight::Normal))
            .style(t::select)
            .menu_style(t::select_menu),
            secondary("Add device", Message::AddDevice, app.operation.busy),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

pub(super) fn device_details(app: &App) -> Element<'_, Message> {
    let Some((index, device)) = app
        .project
        .model
        .devices
        .iter()
        .enumerate()
        .find(|(index, _)| *index == app.ui.selected_device)
    else {
        return note("No device yet. Add one above.");
    };
    let name = match &app.ui.device_name_draft {
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
        danger(
            "Remove this device",
            Message::DeleteDevice(index),
            app.operation.busy
        ),
    ]
    .spacing(10)
    .into()
}

pub(super) fn command_list(app: &App) -> Element<'_, Message> {
    let Some(device) = app.selected_device() else {
        return note("Choose a device first.");
    };
    if device.commands.is_empty() {
        return note("No commands yet. Add at least one below.");
    }
    let referenced = remote_rebirth_core::model::referenced_commands(&app.project.model);
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
        if app.ui.advanced {
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
            app.operation.busy,
        ));
        rows.push(line.into());
        rows.push(rule::horizontal(1.0).style(t::rule_style).into());
    }
    column(rows).spacing(0).into()
}

pub(super) fn add_command_form(app: &App) -> Element<'_, Message> {
    let form: &CommandForm = &app.ui.new_command;
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
                    crate::support::command_formats(),
                    Some(crate::support::command_format_label(&form.format)),
                    |label| Message::NewCommandFormat(crate::support::command_format_from_label(
                        &label
                    ))
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
        secondary("Add command", Message::AddCommand, app.operation.busy),
    ]
    .spacing(8)
    .into()
}

// -- buttons ----------------------------------------------------------------
