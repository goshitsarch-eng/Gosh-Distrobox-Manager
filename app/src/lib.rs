//! Gosh Distrobox Manager — the COSMIC application, as a library.
//!
//! **Why this crate has a lib target (I31 in `docs/migration/PLAN.md`).** Until
//! T16 this package was binary-only: the module tree below was declared in
//! `main.rs`, which made every module private to the binary crate and therefore
//! unreachable from `app/tests/*.rs` — an integration test can only import a
//! *library*. The measured consequence was the single largest verification gap
//! in the migration: of 193 parity rows, **148 rested on `source` tier** (a call
//! path read in the code, not an executed behaviour), against 26 at T1 and 12
//! at T2. Every page-level row was affected, because a page could only be
//! verified by reading it.
//!
//! The split is deliberately minimal: `lib.rs` declares the same modules that
//! `main.rs` used to, and `main.rs` keeps only the entry point. There is one
//! behaviour change and it is the point of the exercise — `app::App`, the page
//! modules and the view helpers are now importable, so a parity row can be
//! pinned by a test that runs it rather than by a comment that describes it.
//!
//! Note on `fl!`: `i18n.rs` marks the macro `#[macro_export]`, which places it
//! at the root of whichever crate declares the module. It is now
//! `gosh_distrobox_manager::fl` (imported inside this library as `crate::fl`),
//! and the binary reaches it through the same path.

pub mod activity;
pub mod app;
pub mod apps_view;
pub mod backups;
pub mod i18n;
pub mod icons;
pub mod images_view;
pub mod message;
pub mod packages;
pub mod settings;
pub mod terminal;
pub mod updates;
pub mod views;
pub mod wizard;
pub mod wizard_view;
