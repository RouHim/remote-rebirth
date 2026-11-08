//! Remote Rebirth desktop GUI.
//!
//! An owner's manual for the Harmony 300: Detect, Back up, build a one-Device Model, edit its
//! Key Map with volume/channel Routing, Compose, Prepare, Commit and Restore.
//!
//! The window is layered: [`app`] owns state and the operation lifecycle, [`handlers`] owns what
//! each message does, [`views`] owns the pages, and [`support`] owns the small conversions.

mod app;
mod handlers;
mod message;
mod operation;
mod support;
#[cfg(test)]
mod tests;
mod theme;
mod views;

use app::App;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Remote Rebirth")
        .theme(App::theme)
        .subscription(App::subscription)
        .font(include_bytes!("../assets/fonts/PublicSans-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/PublicSans-Medium.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/PublicSans-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/PublicSans-Bold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/SourceSerif4-SemiBold.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/SplineSansMono-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/SplineSansMono-Medium.ttf").as_slice())
        .window(iced::window::Settings {
            size: iced::Size::new(1240.0, 820.0),
            min_size: Some(iced::Size::new(1040.0, 680.0)),
            position: iced::window::Position::Centered,
            ..Default::default()
        })
        .run()
}
