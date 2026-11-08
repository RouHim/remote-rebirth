//! The Favourites page: one row per favourite channel button.

use super::widgets::{body, danger, note, primary, section};
use crate::app::App;
use crate::message::Message;
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{Column, column, row, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Length};
use remote_rebirth_core::profile;

pub(super) fn favourites_page(app: &App) -> Element<'_, Message> {
    let capacity = profile::skin_profile(&profile::HARMONY_300, app.project.model.skin)
        .map(|skin| skin.favourite_capacity)
        .unwrap_or(0);
    let mut slots: Vec<Element<'_, Message>> = Vec::new();
    for index in 0..capacity {
        match app.project.model.favourites.get(index) {
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
                        danger(
                            "Remove",
                            Message::DeleteFavourite(index),
                            app.operation.busy
                        ),
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
                Some(&format!("SKIN {} · {} SLOTS", app.project.model.skin, capacity)),
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
                    text_input("Channel number", &app.ui.new_favourite_channel)
                        .on_input(Message::NewFavouriteChannel)
                        .padding(6)
                        .size(11)
                        .font(t::sans(Weight::Normal))
                        .style(t::text_field)
                        .width(Length::Fixed(200.0)),
                    primary("Add", Message::AddFavourite, app.operation.busy),
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
