//! The pages of the owner's manual.
//!
//! The shell is a cloth rail and a paper sheet. Pages are set as spreads:
//! headings in the serif, work text in the sans, codes and counts in the mono,
//! and rules instead of cards. The one repeating signature is the numbered key
//! drawing whose callouts match the work table.

use crate::app::App;
use crate::message::{Message, Page};
use crate::support::{plain_number, step_label};
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{Column, button, column, container, progress_bar, row, rule, text};
use iced::{Alignment, Element, Fill, Length};
use remote_rebirth_core::profile;

mod activities;
mod devices;
mod favourites;
mod keymap;
mod library;
mod remote;
mod settings;
mod widgets;

use self::{
    activities::activities_page,
    devices::devices_page,
    favourites::favourites_page,
    keymap::keymap_page,
    library::library_page,
    remote::remote_page,
    settings::settings_page,
    widgets::{dot, primary, secondary},
};

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

pub(super) fn page_header(app: &App) -> Element<'_, Message> {
    let (title, subtitle) = match app.ui.page {
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
        Page::Library => (
            "IR Library",
            "Search local Flipper files and add a whole device.",
        ),
        Page::Settings => ("Settings", "Display, files, and what this program is."),
    };
    let section = match app.ui.page {
        Page::Remote => "1",
        Page::Devices => "2",
        Page::KeyMap => "3",
        Page::Activities => "4",
        Page::Favourites => "5",
        Page::Library => "6",
        Page::Settings => "7",
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
            secondary("Back up now", Message::Backup, app.operation.busy),
            primary(
                "Review update",
                Message::PrepareWrite,
                app.operation.busy || invalid
            ),
        ]
        .spacing(8),
    ]
    .spacing(18)
    .align_y(Alignment::Start)
    .padding([18, 24])
    .into()
}

pub(super) fn page_body(app: &App) -> Element<'_, Message> {
    match app.ui.page {
        Page::Remote => remote_page(app),
        Page::Devices => devices_page(app),
        Page::KeyMap => keymap_page(app),
        Page::Activities => activities_page(app),
        Page::Favourites => favourites_page(app),
        Page::Library => library_page(app),
        Page::Settings => settings_page(app),
    }
}

pub(super) fn sheet_footer(app: &App) -> Element<'_, Message> {
    let page_number = match app.ui.page {
        Page::Remote => "12",
        Page::Devices => "24",
        Page::KeyMap => "42",
        Page::Activities => "58",
        Page::Favourites => "66",
        Page::Library => "74",
        Page::Settings => "86",
    };
    let saved = if app.project.saved {
        "All changes saved"
    } else {
        "Unsaved changes"
    };
    let container_size = app
        .operation
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

pub(super) fn status_bar(app: &App) -> Element<'_, Message> {
    let identity = match &app.project.remote {
        Some(remote) if app.ui.advanced => format!(
            "{} · Skin {} · firmware {}",
            remote.model,
            plain_number(remote.skin),
            remote.firmware_version.as_deref().unwrap_or("?")
        ),
        Some(remote) => format!("{} connected", remote.model),
        None => "No remote connected".to_string(),
    };
    let validation = match (app.ui.advanced, app.validation()) {
        (true, Some(error)) => error,
        (false, Some(_)) => "Setup incomplete".to_string(),
        (_, None) => "Ready".to_string(),
    };
    let saved = if app.project.saved {
        "All changes saved"
    } else {
        "Unsaved changes"
    };
    let mut items = row![
        dot(if app.project.remote.is_some() {
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

    if let Some((label, fraction)) = &app.operation.progress {
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
    if app.operation.busy && app.operation.committing {
        items = items.push(
            text("Commit in progress — Cancel disabled")
                .font(t::mono(Weight::Normal))
                .size(9.5)
                .color(t::OXIDE_DARK),
        );
    } else if app.operation.busy {
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

pub(super) fn rail(app: &App) -> Element<'_, Message> {
    let remote_state = if app.project.remote.is_some() {
        "READY"
    } else {
        "OFFLINE"
    };
    let keys: usize = app
        .project
        .model
        .devices
        .iter()
        .map(|device| device.keys.len())
        .sum();
    let capacity = profile::skin_profile(&profile::HARMONY_300, app.project.model.skin)
        .map(|skin| skin.favourite_capacity)
        .unwrap_or(0);
    let items: Vec<Element<'_, Message>> = vec![
        nav_item(app, Page::Remote, "Remote", remote_state.to_string()),
        nav_item(
            app,
            Page::Devices,
            "My Devices",
            app.project.model.devices.len().to_string(),
        ),
        nav_item(app, Page::KeyMap, "Buttons", keys.to_string()),
        nav_item(
            app,
            Page::Activities,
            "Watch TV",
            app.project.model.activity.steps.len().to_string(),
        ),
        nav_item(
            app,
            Page::Favourites,
            "Favourites",
            format!("{}/{}", app.project.model.favourites.len(), capacity),
        ),
        nav_item(
            app,
            Page::Library,
            "IR Library",
            app.library.devices.len().to_string(),
        ),
        nav_item(app, Page::Settings, "Settings", String::new()),
    ];
    let remote_line = match &app.project.remote {
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
                dot(if app.project.remote.is_some() {
                    t::GRASS
                } else {
                    t::FAINT
                }),
                text(if app.project.remote.is_some() {
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

pub(super) fn nav_item<'a>(
    app: &'a App,
    page: Page,
    label: &'static str,
    count: String,
) -> Element<'a, Message> {
    let active = app.ui.page == page;
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
    if !app.operation.busy {
        item = item.on_press(Message::Page(page));
    }
    item.style(if active { t::rail_active } else { t::rail })
        .into()
}
