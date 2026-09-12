//! Localization via `i18n-embed` + Fluent (S8/T15), same pattern as the other
//! COSMIC apps in this family.
//!
//! This is a *crate-local* loader, deliberately not libcosmic's. libcosmic
//! exports its own `fl!` over its own `LANGUAGE_LOADER`, which embeds
//! libcosmic's `i18n/` — using it would resolve only libcosmic's own message
//! ids and silently fail on ours. So `app/` embeds its own catalogue and
//! defines its own `fl!`, shadowing anything re-exported.
//!
//! `#[macro_export]` puts `fl!` at the crate root, which is why `main.rs` (the
//! binary root) can call it bare while every submodule needs `use crate::fl;`.

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DefaultLocalizer, DesktopLanguageRequester, LanguageLoader, Localizer};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("failed to load fallback language (is app/i18n/en/*.ftl present?)");
    loader
});

/// Apply the system-requested languages. Call once before `app::run`.
pub fn init() {
    let localizer = DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations);
    let requested = DesktopLanguageRequester::requested_languages();
    if let Err(e) = localizer.select(&requested) {
        tracing::warn!("failed to select requested languages: {e}");
    }
}

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    }};
    ($message_id:literal, $($args:expr),*) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args),*)
    }};
}
