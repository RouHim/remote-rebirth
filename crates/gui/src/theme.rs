//! The owner's-manual visual system: paper, one oxide spot, and cloth.
//!
//! Every colour, border and state used by the GUI lives here so the pages stay
//! typographic and the components share one material language.

use iced::border::{self, Radius};
use iced::font::{Family, Weight};
use iced::overlay::menu;
use iced::widget::{
    button, checkbox as checkbox_widget, container, pick_list, progress_bar, rule,
    scrollable as scrollable_widget, text_input,
};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector};

pub const PAPER: Color = Color::from_rgb8(0xF6, 0xF0, 0xE3);
pub const PAPER_2: Color = Color::from_rgb8(0xFB, 0xF7, 0xED);
pub const HOVER: Color = Color::from_rgb8(0xEF, 0xE6, 0xD4);
pub const PRESSED: Color = Color::from_rgb8(0xE7, 0xDC, 0xC8);
pub const DESK: Color = Color::from_rgb8(0xE3, 0xD8, 0xC4);
pub const INK: Color = Color::from_rgb8(0x23, 0x21, 0x1C);
pub const MUTED: Color = Color::from_rgb8(0x6E, 0x64, 0x55);
pub const FAINT: Color = Color::from_rgb8(0x76, 0x6B, 0x5B);
pub const RULE: Color = Color::from_rgb8(0xD2, 0xC6, 0xB0);
pub const OXIDE: Color = Color::from_rgb8(0xB3, 0x48, 0x24);
pub const OXIDE_DARK: Color = Color::from_rgb8(0x8E, 0x3A, 0x1F);
pub const OXIDE_SOFT: Color = Color::from_rgb8(0xF0, 0xDD, 0xD2);
pub const GRASS: Color = Color::from_rgb8(0x5B, 0x71, 0x53);
pub const COBALT: Color = Color::from_rgb8(0x2F, 0x5A, 0x7A);
pub const COVER: Color = Color::from_rgb8(0x8C, 0x3F, 0x25);
pub const COVER_DARK: Color = Color::from_rgb8(0x73, 0x30, 0x1C);
pub const COVER_LINE: Color = Color::from_rgb8(0xA8, 0x5B, 0x3E);
pub const COVER_TEXT: Color = Color::from_rgb8(0xEE, 0xD0, 0xBC);
pub const CREAM: Color = Color::from_rgb8(0xFF, 0xF7, 0xE8);

pub const SERIF: &str = "Source Serif 4";
pub const SANS: &str = "Public Sans";
pub const MONO: &str = "Spline Sans Mono";

pub fn serif() -> Font {
    Font::with_name(SERIF)
}

pub fn sans(weight: Weight) -> Font {
    Font {
        family: Family::Name(SANS),
        weight,
        ..Font::DEFAULT
    }
}

pub fn mono(weight: Weight) -> Font {
    Font {
        family: Family::Name(MONO),
        weight,
        ..Font::DEFAULT
    }
}

fn flat(radius: f32) -> Radius {
    Radius::from(radius)
}

fn border(width: f32, color: Color) -> Border {
    Border {
        width,
        color,
        radius: flat(0.0),
    }
}

pub fn primary(_theme: &Theme, status: button::Status) -> button::Style {
    let active = button::Style {
        background: Some(Background::Color(OXIDE)),
        text_color: CREAM,
        border: Border {
            color: OXIDE,
            ..border(1.0, OXIDE)
        },
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(OXIDE_DARK)),
            border: Border {
                color: OXIDE_DARK,
                ..active.border
            },
            ..active
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(OXIDE_DARK)),
            border: Border {
                color: OXIDE_DARK,
                ..active.border
            },
            ..active
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(PRESSED)),
            text_color: FAINT,
            border: Border {
                color: RULE,
                ..active.border
            },
            ..active
        },
        button::Status::Active => active,
    }
}

pub fn secondary(_theme: &Theme, status: button::Status) -> button::Style {
    let active = button::Style {
        background: None,
        text_color: INK,
        border: border(1.0, RULE),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(HOVER)),
            border: border(1.0, FAINT),
            ..active
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(PRESSED)),
            border: border(1.0, INK),
            ..active
        },
        button::Status::Disabled => button::Style {
            text_color: FAINT,
            border: border(1.0, RULE),
            ..active
        },
        button::Status::Active => active,
    }
}

pub fn ghost(_theme: &Theme, status: button::Status) -> button::Style {
    let active = button::Style {
        background: None,
        text_color: INK,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(HOVER)),
            ..active
        },
        button::Status::Disabled => button::Style {
            text_color: FAINT,
            ..active
        },
        button::Status::Active => active,
    }
}

pub fn danger(_theme: &Theme, status: button::Status) -> button::Style {
    let active = button::Style {
        background: None,
        text_color: OXIDE_DARK,
        border: border(1.0, RULE),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(OXIDE_SOFT)),
            border: border(1.0, OXIDE),
            ..active
        },
        button::Status::Disabled => button::Style {
            text_color: FAINT,
            ..active
        },
        button::Status::Active => active,
    }
}

pub fn rail(_theme: &Theme, status: button::Status) -> button::Style {
    let active = button::Style {
        background: None,
        text_color: CREAM,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(COVER_DARK)),
            ..active
        },
        button::Status::Disabled => button::Style {
            text_color: COVER_LINE,
            ..active
        },
        button::Status::Active => active,
    }
}

pub fn rail_active(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(COVER_DARK)),
        text_color: CREAM,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let base = if active {
            button::Style {
                background: Some(Background::Color(PAPER)),
                text_color: INK,
                border: border(1.0, INK),
                shadow: Shadow::default(),
                snap: true,
            }
        } else {
            button::Style {
                background: None,
                text_color: MUTED,
                border: border(1.0, RULE),
                shadow: Shadow::default(),
                snap: true,
            }
        };
        match status {
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(HOVER)),
                text_color: INK,
                ..base
            },
            button::Status::Disabled => button::Style {
                text_color: FAINT,
                ..base
            },
            button::Status::Active => base,
        }
    }
}

pub fn row_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let base = button::Style {
            background: if selected {
                Some(Background::Color(OXIDE_SOFT))
            } else {
                None
            },
            text_color: INK,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        };
        match status {
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(HOVER)),
                ..base
            },
            button::Status::Disabled => button::Style {
                text_color: FAINT,
                ..base
            },
            button::Status::Active => base,
        }
    }
}

pub fn key(
    selected: bool,
    inherited: bool,
    empty: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let base = if selected {
            button::Style {
                background: Some(Background::Color(OXIDE)),
                text_color: CREAM,
                border: border(1.0, OXIDE),
                shadow: Shadow::default(),
                snap: true,
            }
        } else if inherited {
            button::Style {
                background: Some(Background::Color(PAPER_2)),
                text_color: COBALT,
                border: border(1.0, RULE),
                shadow: Shadow::default(),
                snap: true,
            }
        } else if empty {
            button::Style {
                background: None,
                text_color: FAINT,
                border: border(1.0, RULE),
                shadow: Shadow::default(),
                snap: true,
            }
        } else {
            button::Style {
                background: Some(Background::Color(PAPER_2)),
                text_color: INK,
                border: border(1.0, INK),
                shadow: Shadow::default(),
                snap: true,
            }
        };
        match status {
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(if selected { OXIDE_DARK } else { HOVER })),
                border: border(1.0, if selected { OXIDE_DARK } else { INK }),
                ..base
            },
            button::Status::Disabled => button::Style {
                text_color: FAINT,
                ..base
            },
            button::Status::Active => base,
        }
    }
}

pub fn bay(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let base = button::Style {
            background: if active {
                Some(Background::Color(OXIDE_SOFT))
            } else {
                Some(Background::Color(PAPER_2))
            },
            text_color: INK,
            border: border(1.0, if active { OXIDE } else { RULE }),
            shadow: Shadow::default(),
            snap: true,
        };
        match status {
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(HOVER)),
                border: border(1.0, FAINT),
                ..base
            },
            button::Status::Disabled => button::Style {
                text_color: FAINT,
                ..base
            },
            button::Status::Active => base,
        }
    }
}

pub fn sheet(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PAPER)),
        border: border(1.0, RULE),
        shadow: Shadow {
            color: Color::from_rgba(0.15, 0.12, 0.08, 0.18),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        },
        ..Default::default()
    }
}

pub fn desk(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(DESK)),
        ..Default::default()
    }
}

pub fn rail_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(COVER)),
        ..Default::default()
    }
}

pub fn plate(_theme: &Theme) -> container::Style {
    container::Style {
        border: border(1.0, RULE),
        ..Default::default()
    }
}

pub fn errata(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(OXIDE_SOFT)),
        border: border(1.0, OXIDE),
        ..Default::default()
    }
}

pub fn stamp(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        border: border(1.0, color),
        ..Default::default()
    }
}

pub fn progress(_theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(RULE),
        bar: Background::Color(OXIDE),
        border: border(0.0, RULE),
    }
}

pub fn rule_style(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: RULE,
        radius: flat(0.0),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn ink_rule(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: INK,
        radius: flat(0.0),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn text_field(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let active = text_input::Style {
        background: Background::Color(PAPER_2),
        border: border(1.0, RULE),
        icon: MUTED,
        placeholder: FAINT,
        value: INK,
        selection: OXIDE_SOFT,
    };
    match status {
        text_input::Status::Hovered => text_input::Style {
            border: border(1.0, FAINT),
            ..active
        },
        text_input::Status::Focused { .. } => text_input::Style {
            border: border(1.0, OXIDE),
            ..active
        },
        text_input::Status::Disabled => text_input::Style {
            background: Background::Color(PRESSED),
            value: FAINT,
            ..active
        },
        text_input::Status::Active => active,
    }
}

pub fn select(_theme: &Theme, _status: pick_list::Status) -> pick_list::Style {
    pick_list::Style {
        text_color: INK,
        placeholder_color: FAINT,
        handle_color: MUTED,
        background: Background::Color(PAPER_2),
        border: border(1.0, RULE),
    }
}

pub fn select_menu(_theme: &Theme) -> menu::Style {
    menu::Style {
        background: Background::Color(PAPER_2),
        border: border(1.0, INK),
        text_color: INK,
        selected_text_color: CREAM,
        selected_background: Background::Color(OXIDE),
        shadow: Shadow::default(),
    }
}

pub fn checkbox(_theme: &Theme, status: checkbox_widget::Status) -> checkbox_widget::Style {
    let is_checked = match status {
        checkbox_widget::Status::Active { is_checked }
        | checkbox_widget::Status::Hovered { is_checked }
        | checkbox_widget::Status::Disabled { is_checked } => is_checked,
    };
    checkbox_widget::Style {
        background: Background::Color(if is_checked { OXIDE } else { PAPER_2 }),
        icon_color: CREAM,
        border: border(1.0, if is_checked { OXIDE } else { INK }),
        text_color: Some(match status {
            checkbox_widget::Status::Disabled { .. } => FAINT,
            _ => INK,
        }),
    }
}

pub fn scrollable(_theme: &Theme, _status: scrollable_widget::Status) -> scrollable_widget::Style {
    scrollable_widget::Style {
        container: container::Style::default(),
        vertical_rail: scrollable_widget::Rail {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default(),
            scroller: scrollable_widget::Scroller {
                background: Background::Color(RULE),
                border: border::rounded(2),
            },
        },
        horizontal_rail: scrollable_widget::Rail {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default(),
            scroller: scrollable_widget::Scroller {
                background: Background::Color(RULE),
                border: border::rounded(2),
            },
        },
        gap: None,
        auto_scroll: scrollable_widget::AutoScroll {
            background: Background::Color(PAPER),
            border: Border::default(),
            shadow: Shadow::default(),
            icon: INK,
        },
    }
}
