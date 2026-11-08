//! The IR Library page: a local Flipper file browser.

use super::widgets::{body, data_line, note, paint, secondary, section};
use crate::app::App;
use crate::message::Message;
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{Column, button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Length};

pub(super) fn library_page(app: &App) -> Element<'_, Message> {
    let library_error: Element<'_, Message> = match &app.library.error {
        Some(error) => text(error.to_string())
            .font(t::sans(Weight::Normal))
            .size(10.5)
            .color(t::OXIDE_DARK)
            .into(),
        None => iced::widget::Space::new().height(0).into(),
    };
    let total_commands: usize = app
        .library
        .devices
        .iter()
        .map(|device| device.commands.len())
        .sum();
    let library_status: Element<'_, Message> = if app.library.status.is_empty() {
        iced::widget::Space::new().height(0).into()
    } else {
        note(&app.library.status)
    };
    let source = section(
        "Source",
        Some("LOCAL FOLDER"),
        column![
            body("Point at a Flipper-IRDB checkout or a folder of .ir files. No network request is made."),
            row![
                text_input("Folder with Flipper .ir files", &app.library.dir)
                    .on_input(Message::LibraryPath)
                    .padding(6)
                    .size(10)
                    .font(t::mono(Weight::Normal))
                    .style(t::text_field)
                    .width(Fill),
                secondary("Scan folder", Message::ScanLibrary, false),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            library_status,
            note(format!(
                "{} devices · {} commands indexed",
                app.library.devices.len(),
                total_commands
            )),
            library_error,
        ]
        .spacing(7)
        .into(),
    );
    let search = section(
        "Search",
        Some("BRAND · MODEL · COMMAND"),
        column![
            text_input("Search brand, model or command", &app.library.search)
                .on_input(Message::LibrarySearch)
                .padding(6)
                .size(10)
                .font(t::sans(Weight::Normal))
                .style(t::text_field),
            library_results(app),
        ]
        .spacing(6)
        .height(Fill)
        .into(),
    );
    let left = column![source, search]
        .spacing(18)
        .height(Fill)
        .width(Length::FillPortion(2));
    let right = container(section(
        "Selected device",
        Some("COMMANDS"),
        library_detail(app),
    ))
    .width(Length::FillPortion(3));

    row![
        left,
        container(iced::widget::Space::new())
            .width(Length::Fixed(1.0))
            .height(Fill)
            .style(|_| container::Style {
                background: Some(paint(t::RULE)),
                ..Default::default()
            }),
        right,
    ]
    .spacing(14)
    .height(Fill)
    .padding([14, 24])
    .into()
}

pub(super) fn library_results(app: &App) -> Element<'_, Message> {
    let query = app.library.search.to_lowercase();
    let filtered: Vec<(usize, &crate::message::LibraryDevice)> = app
        .library
        .devices
        .iter()
        .enumerate()
        .filter(|(_, device)| {
            query.is_empty()
                || device.brand.to_lowercase().contains(&query)
                || device.model.to_lowercase().contains(&query)
                || device
                    .commands
                    .iter()
                    .any(|command| command.to_lowercase().contains(&query))
        })
        .take(200)
        .collect();
    if filtered.is_empty() {
        return note("No indexed device matches this search.");
    }
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for (index, device) in filtered {
        let selected = app.library.selected == Some(index);
        let content = row![
            text(&device.brand)
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED)
                .width(Length::Fixed(86.0)),
            text(&device.model)
                .font(t::sans(Weight::Semibold))
                .size(10.5)
                .color(t::INK)
                .width(Fill),
            text(format!("{} commands", device.commands.len()))
                .font(t::mono(Weight::Normal))
                .size(7.5)
                .color(t::MUTED),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        rows.push(
            button(content)
                .width(Fill)
                .padding([5, 6])
                .style(t::row_button(selected))
                .on_press(Message::SelectLibraryDevice(index))
                .into(),
        );
        rows.push(rule::horizontal(1.0).style(t::rule_style).into());
    }
    scrollable(Column::with_children(rows))
        .height(Fill)
        .style(t::scrollable)
        .into()
}

pub(super) fn library_detail(app: &App) -> Element<'_, Message> {
    let Some(device) = app
        .library
        .selected
        .and_then(|index| app.library.devices.get(index))
    else {
        return note("Choose a library device from the results.");
    };
    let target = app.selected_device().map(|device| device.name.clone());
    let add_label = target
        .as_ref()
        .map(|name| format!("Add to {name}"))
        .unwrap_or_else(|| "Add to device".into());
    let commands: Vec<Element<'_, Message>> = device
        .commands
        .iter()
        .map(|name| {
            text(name)
                .font(t::sans(Weight::Normal))
                .size(10.5)
                .color(t::INK)
                .into()
        })
        .collect();
    let mut content = column![
        text(format!("{} · {}", device.brand, device.model))
            .font(t::serif())
            .size(18)
            .color(t::INK),
        note(format!(
            "{} commands. The full set is imported; unused commands stay available in the Model.",
            device.commands.len()
        )),
    ]
    .spacing(6);
    if app.ui.advanced {
        content = content.push(data_line("File", &device.path.display().to_string()));
    }
    content = content.push(
        scrollable(Column::with_children(commands))
            .height(Fill)
            .style(t::scrollable),
    );
    content = content.push(
        button(text(add_label).size(11).font(t::sans(Weight::Semibold)))
            .on_press_maybe(
                (target.is_some() && !device.commands.is_empty() && !app.operation.busy)
                    .then_some(Message::AddLibraryDevice),
            )
            .padding([6, 12])
            .style(t::primary),
    );
    if target.is_none() {
        content = content.push(note("Choose a device on My Devices first."));
    }
    if !app.library.status.is_empty() {
        content = content.push(note(&app.library.status));
    }
    if let Some(error) = &app.library.error {
        content = content.push(
            text(error.to_string())
                .font(t::sans(Weight::Normal))
                .size(10.5)
                .color(t::OXIDE_DARK),
        );
    }
    content.height(Fill).into()
}
