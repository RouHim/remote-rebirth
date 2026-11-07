//! Small parts shared by more than one page: fields, notes, stamps and buttons.

use crate::app::App;
use crate::message::Message;
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{button, column, container, row, rule, text};
use iced::{Alignment, Color, Element, Fill, Length};
use remote_rebirth_core::gui_api;

const NO_COMMAND: &str = "—";

pub(super) fn section<'a>(
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

pub(super) fn procedure<'a>(
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

pub(super) fn field<'a>(label: &'a str, control: Element<'a, Message>) -> Element<'a, Message> {
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

pub(super) fn shortcut_row(keys: &'static str, action: &'static str) -> Element<'static, Message> {
    row![
        text(keys)
            .font(t::mono(Weight::Medium))
            .size(8.5)
            .color(t::OXIDE_DARK)
            .width(Length::Fixed(132.0)),
        text(action)
            .font(t::sans(Weight::Normal))
            .size(10.5)
            .color(t::INK),
    ]
    .spacing(8)
    .into()
}

pub(super) fn data_line(label: &str, value: &str) -> Element<'static, Message> {
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

pub(super) fn mono_label(value: &str) -> Element<'static, Message> {
    text(value.to_uppercase())
        .font(t::mono(Weight::Medium))
        .size(8)
        .color(t::MUTED)
        .into()
}

pub(super) fn body(value: impl ToString) -> Element<'static, Message> {
    text(value.to_string())
        .font(t::sans(Weight::Normal))
        .size(11.5)
        .color(t::INK)
        .into()
}

pub(super) fn note(value: impl ToString) -> Element<'static, Message> {
    text(value.to_string())
        .font(t::sans(Weight::Normal))
        .size(10.5)
        .color(t::MUTED)
        .into()
}

pub(super) fn stamp(label: &str, color: Color) -> Element<'static, Message> {
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

pub(super) fn errata(app: &App, error: &str) -> Element<'static, Message> {
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
    if app.ui.advanced {
        detail = detail.push(
            text(error.to_string())
                .font(t::mono(Weight::Normal))
                .size(8.5)
                .color(t::MUTED),
        );
        detail = detail.push(data_line(
            "Activity log",
            &app.project
                .state_dir
                .join("journal.jsonl")
                .display()
                .to_string(),
        ));
    }
    let mut actions = row![
        ghost(
            if app.ui.advanced {
                "Hide details"
            } else {
                "Technical details"
            },
            Message::ToggleAdvanced(!app.ui.advanced),
            false
        ),
        ghost("Dismiss", Message::DismissError, false),
    ]
    .spacing(4);
    if let Some(path) = &app.project.backup_path {
        actions = actions.push(ghost(
            "Restore this backup",
            Message::UseBackup(path.clone()),
            app.operation.busy || backup_row(path, false).1 != "ok",
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

pub(super) fn friendly_error(error: &str) -> String {
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

pub(super) fn primary(
    label: &'static str,
    message: Message,
    disabled: bool,
) -> Element<'static, Message> {
    button(text(label).size(11).font(t::sans(Weight::Semibold)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([6, 12])
        .style(t::primary)
        .into()
}

pub(super) fn secondary(
    label: &'static str,
    message: Message,
    disabled: bool,
) -> Element<'static, Message> {
    button(text(label).size(10.5).font(t::sans(Weight::Medium)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([6, 10])
        .style(t::secondary)
        .into()
}

pub(super) fn ghost(
    label: &'static str,
    message: Message,
    disabled: bool,
) -> Element<'static, Message> {
    button(text(label).size(10).font(t::sans(Weight::Medium)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([4, 6])
        .style(t::ghost)
        .into()
}

pub(super) fn danger(
    label: &'static str,
    message: Message,
    disabled: bool,
) -> Element<'static, Message> {
    button(text(label).size(10).font(t::sans(Weight::Medium)))
        .on_press_maybe((!disabled).then_some(message))
        .padding([4, 6])
        .style(t::danger)
        .into()
}
pub(super) fn dot(color: Color) -> Element<'static, Message> {
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

pub(super) fn paint(color: Color) -> iced::Background {
    iced::Background::Color(color)
}

// -- remote -----------------------------------------------------------------

pub(super) fn backup_row(path: &std::path::Path, advanced: bool) -> (String, String) {
    let sidecar = gui_api::backup_sidecar(path);
    let Ok(summary) = gui_api::backup_summary(&sidecar) else {
        return ("Saved backup".into(), "unknown".into());
    };
    let detail = if advanced {
        format!(
            "{} · Skin {} · {} · {} B",
            summary.model,
            summary
                .skin
                .map(|skin| skin.to_string())
                .unwrap_or_else(|| "?".into()),
            summary.validation,
            summary.size
        )
    } else if summary.validation == "ok" {
        "Saved backup · valid".into()
    } else {
        "Saved backup · damaged".into()
    };
    (detail, summary.validation)
}

pub(super) fn command_options(app: &App, device_name: &str, current: Option<&str>) -> Vec<String> {
    let mut options = vec![NO_COMMAND.to_string()];
    if let Some(device) = app
        .project
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

pub(super) fn command_selected(current: Option<&str>) -> String {
    current
        .filter(|value| !value.is_empty())
        .unwrap_or(NO_COMMAND)
        .to_string()
}

pub(super) fn command_choice(value: String) -> String {
    if value == NO_COMMAND {
        String::new()
    } else {
        value
    }
}
