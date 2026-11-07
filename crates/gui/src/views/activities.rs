//! The Watch TV page: the ordered activity steps.

use super::widgets::{
    body, command_choice, command_options, command_selected, danger, field, note, secondary,
    section,
};
use crate::app::App;
use crate::message::Message;
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{Column, column, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Length};

pub(super) fn activities_page(app: &App) -> Element<'_, Message> {
    let names = app.device_names();
    let mut steps: Vec<Element<'_, Message>> = Vec::new();
    for (index, step) in app.project.model.activity.steps.iter().enumerate() {
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
                danger("Delete", Message::DeleteStep(index), app.operation.busy),
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
    let add_power = command_options(app, &app.ui.activity_device, Some(&app.ui.activity_power));
    let add_input = command_options(app, &app.ui.activity_device, Some(&app.ui.activity_input));

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
                            Some(app.project.model.activity.channel_device.clone()),
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
                            Some(app.ui.activity_device.clone()),
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
                                Some(command_selected(Some(&app.ui.activity_power))),
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
                                Some(command_selected(Some(&app.ui.activity_input))),
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
                            text_input("Wait (ms)", &app.ui.activity_delay)
                                .on_input(Message::ActivityDelay)
                                .padding(6)
                                .size(10)
                                .font(t::mono(Weight::Normal))
                                .style(t::text_field)
                                .into()
                        ),
                    ]
                    .spacing(8),
                    secondary("Add step", Message::AddStep, app.operation.busy),
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
