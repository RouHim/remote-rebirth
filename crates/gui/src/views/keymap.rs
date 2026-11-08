//! The Buttons page: the key drawing, the key table and the command index.

use super::widgets::{ghost, mono_label, note, paint};
use crate::app::App;
use crate::message::Message;
use crate::support::{humanize, none_if_placeholder, routed_target};
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{
    Column, Row, button, column, container, pick_list, row, rule, scrollable, text, text_input,
};
use iced::{Alignment, Element, Fill, Length};
use remote_rebirth_core::profile;

const KEY_COLUMNS: usize = 3;

pub(super) fn keymap_page(app: &App) -> Element<'_, Message> {
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

pub(super) fn device_tabs(app: &App) -> Element<'_, Message> {
    let names = app.device_names();
    let mut tabs: Vec<Element<'_, Message>> = Vec::new();
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
            && !app.operation.busy
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

pub(super) fn routing_band(app: &App) -> Element<'_, Message> {
    let names = app.device_names();
    let mut options = vec!["Each device itself".to_string()];
    options.extend(names);
    let volume = app
        .project
        .model
        .routing
        .as_ref()
        .and_then(|routing| routing.volume.clone())
        .unwrap_or_else(|| "Each device itself".into());
    let channel = app
        .project
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

pub(super) fn route_control<'a>(
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

pub(super) fn remote_diagram(app: &App) -> Element<'_, Message> {
    let selected = app.ui.selected_key.as_deref();
    let remote = app
        .project
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
                && !app.operation.busy
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
            let inherited = routed_target(&app.project.model, binding.name, &device_name).is_some();
            let empty = app
                .selected_device()
                .map(|device| !device.keys.contains_key(binding.name))
                .unwrap_or(true);
            let label_color = if is_selected {
                t::CREAM
            } else if inherited {
                t::COBALT
            } else {
                t::INK
            };
            let content = row![
                text(format!("{index:02}"))
                    .font(t::mono(Weight::Normal))
                    .size(6.5)
                    .color(if is_selected { t::CREAM } else { t::OXIDE }),
                text(key_short(binding.name))
                    .font(t::sans(Weight::Medium))
                    .size(7.2)
                    .color(label_color),
            ]
            .spacing(4)
            .align_y(Alignment::Center);
            let mut key = button(content)
                .width(Fill)
                .height(Length::Fixed(20.0))
                .padding([0, 4])
                .style(t::key(is_selected, inherited, empty));
            if !app.operation.busy && !inherited {
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

pub(super) fn key_short(name: &str) -> &'static str {
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

pub(super) fn selected_key_summary(app: &App) -> Element<'_, Message> {
    let Some(key) = &app.ui.selected_key else {
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
    let inherited = routed_target(&app.project.model, key, &device.name);
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
        ghost(
            "Clear",
            Message::ClearKey,
            app.operation.busy || inherited.is_some()
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn key_table(app: &App) -> Element<'_, Message> {
    let Some(device) = app.selected_device() else {
        return note("No device selected.");
    };
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for (index, binding) in profile::HARMONY_300.key_layout.iter().enumerate() {
        let inherited = routed_target(&app.project.model, binding.name, &device.name);
        let (command, target, state, state_color) = if let Some(target) = &inherited {
            let command = app
                .project
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
        let selected = app.ui.selected_key.as_deref() == Some(binding.name);
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
        if !app.operation.busy && inherited.is_none() {
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

pub(super) fn command_index(app: &App) -> Element<'_, Message> {
    let Some(device) = app.selected_device() else {
        return note("No device selected.");
    };
    let filter = app.ui.command_filter.to_lowercase();
    let referenced = remote_rebirth_core::model::referenced_commands(&app.project.model);
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
        if app.ui.advanced {
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
        if app.ui.selected_key.is_some() && !app.operation.busy {
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
        text_input("Search the index", &app.ui.command_filter)
            .on_input(Message::CommandFilter)
            .on_submit(Message::AssignFirstCommand)
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
