//! Gosh Distrobox Manager — libcosmic native binary (S5, T3).
//!
//! Entry point only: logging init (moved out of the dropped `api.rs::init_app`)
//! plus `cosmic::app::run`. Everything else lives in `app.rs`/`message.rs`.

mod app;
mod icons;
mod images_view;
mod message;
mod views;
mod wizard;
mod wizard_view;

use cosmic::app::Settings;
use cosmic::iced::Size;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Was `api.rs::init_app` (`#[frb(init)]`); the FRB shim is deleted in S7
    // (T14), and tracing init belongs to the binary, not the core lib.
    //
    // Explicit `EnvFilter` (not the `fmt` default): both default to INFO
    // today (`fmt::Subscriber::DEFAULT_MAX_LEVEL`), but only the filter form
    // honours RUST_LOG (e.g. RUST_LOG=debug) — the plain default ignores the
    // environment entirely. The D17 SMOKE_READY readiness line is an `info!`
    // in `App::view`; keeping it visible by default keeps the smoke test's
    // core assertion falsifiable. `try_init` (not `init`): tests that pull in
    // the binary share the process-global subscriber; a second init must not
    // panic.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let settings = Settings::default().size(Size::new(1024., 768.));
    cosmic::app::run::<app::App>(settings, ())?;
    Ok(())
}
