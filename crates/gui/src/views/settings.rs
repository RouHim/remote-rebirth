//! The Settings page: display preferences, files and the colophon.

use super::widgets::{body, data_line, note, secondary, section, shortcut_row};
use crate::app::App;
use crate::message::Message;
use crate::theme as t;
use iced::font::Weight;
use iced::widget::{checkbox, column, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Length};

pub(super) fn settings_page(app: &App) -> Element<'_, Message> {
    let display = section(
        "Display",
        Some("PREFERENCES"),
        column![
            body("Changes are saved automatically."),
            checkbox(app.ui.advanced)
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
    let keyboard = section(
        "Keyboard",
        Some("SHORTCUTS"),
        column![
            shortcut_row("Ctrl+1…7", "Chapters"),
            shortcut_row("Ctrl+PageUp/PageDown", "Previous or next chapter"),
            shortcut_row("Ctrl+B", "Back up now"),
            shortcut_row("Ctrl+R", "Review update"),
            shortcut_row("Ctrl+F", "Find remote"),
            shortcut_row("Ctrl+Enter", "Assign the first matching command"),
            shortcut_row("Ctrl+Alt+←/→", "Previous or next device"),
            shortcut_row("Ctrl+Alt+↑/↓", "Previous or next key"),
            shortcut_row("Ctrl+Alt+Backspace", "Clear the selected key"),
            shortcut_row("Esc", "Dismiss the error or clear the selection"),
        ]
        .spacing(5)
        .into(),
    );
    let left = column![display, about, keyboard].spacing(18).width(Fill);

    let right: Element<'_, Message> = if app.ui.advanced {
        column![
            section(
                "Remote setup",
                Some("FILES"),
                column![
                    data_line("Data directory", &app.project.data_dir.display().to_string()),
                    data_line("Model", &app.project.model_path.display().to_string()),
                    data_line("Container", &app.project.container_path.display().to_string()),
                    data_line("Backups", &app.project.backups_dir.display().to_string()),
                    row![
                        text("Skin")
                            .font(t::mono(Weight::Normal))
                            .size(9)
                            .color(t::MUTED)
                            .width(Length::Fixed(90.0)),
                        pick_list(vec![78u8, 79], Some(app.project.model.skin), Message::SkinChanged)
                            .padding(5)
                            .text_size(10)
                            .font(t::sans(Weight::Normal))
                            .style(t::select)
                            .menu_style(t::select_menu),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        secondary("Save now", Message::Save, app.operation.busy),
                        secondary("Reload", Message::Reload, app.operation.busy),
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
                    text_input("Path to Model JSON", &app.project.model_transfer_path)
                        .on_input(Message::TransferPath)
                        .padding(6)
                        .size(10)
                        .font(t::mono(Weight::Normal))
                        .style(t::text_field),
                    row![
                        secondary("Export Model", Message::ExportModel, app.operation.busy),
                        secondary("Import Model", Message::ImportModel, app.operation.busy),
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
