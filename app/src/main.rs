//! Gosh Distrobox Manager — libcosmic native binary (S5, T3).
//!
//! Entry point only: logging init (moved out of the dropped `api.rs::init_app`)
//! plus `cosmic::app::run`. Everything else lives in `app.rs`/`message.rs`.

mod app;
mod message;

use cosmic::app::Settings;
use cosmic::iced::Size;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Was `api.rs::init_app` (`#[frb(init)]`); the FRB shim is deleted in S7
    // (T14), and tracing init belongs to the binary, not the core lib.
    let _ = tracing_subscriber::fmt::try_init();

    let settings = Settings::default().size(Size::new(1024., 768.));
    cosmic::app::run::<app::App>(settings, ())?;
    Ok(())
}
