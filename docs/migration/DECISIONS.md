# Migration decisions

Standing tie-break priority (project brief, applies to all decisions below):
parity with the shipped app → correct Flatpak sandbox behavior → accessibility →
simplicity/maintainability → COSMIC conventions.

## D1 — Parity source is the Flutter app, not GTK4

- **Question:** The brief assumes a GTK4 codebase; the repo ships Flutter + Rust. What counts as "the current app" for feature parity?
- **Options:** (a) Flutter UI in `lib/`; (b) upstream DistroShelf GTK4 code.
- **Choice:** (a). The Flutter bundle is what the RPM spec, desktop file, and metainfo ship.
- **Why:** Parity must be measured against what users run. Recorded in memory as
  [[cosmic-migration-flutter-source]].

## D2 — libcosmic pinned by git SHA, never crates.io

- **Question:** How to depend on libcosmic?
- **Options:** (a) crates.io `cosmic`; (b) tag `v0.12`; (c) `branch = "master"`; (d) pinned `rev`.
- **Choice:** (d) `rev = "a401af8b1c54a8abd393b8c5b7c8809402f83850"` (2026-09-10, v1.0.0).
- **Why:** (a) is an unrelated squat crate; (b) is a stale 2024 pre-1.0 API; (c) is
  unreproducible against a daily-committing repo. Revisit the pin only deliberately.

## D3 — Backend moves with history, Flutter tree removed last

- **Question:** How does `rust/` become the core crate, and when does Flutter code go away?
- **Options:** (a) copy backend into a new crate; (b) `git mv rust core`, strip FRB shims
  in small green steps, delete the Flutter tree in the final cleanup task.
- **Choice:** (b).
- **Why:** Preserves history; the "buildable after every task" guarantee applies to the new
  Cargo workspace (`core` + `app`), not the retired Flutter shell (no Dart SDK exists here,
  so the Flutter tree is unverifiable anyway).

## D4 — Architecture findings accepted pending devil's advocate review

- **Question:** Accept the architecture teammate's verified API traps into the plan?
- **Options:** (a) trust-but-verify during review; (b) re-verify now.
- **Choice:** (a) — carried as review items: `Application::init` (no `new()`),
  `iced::Task<cosmic::Action<M>>` vs bare `cosmic::Task`, no `cosmic::Subscription`
  (use `cosmic::iced::Subscription`), and no `tokio::spawn` from `update()`
  (use `cosmic::task::future`).
- **Why:** Single review pass over all three docs is cheaper than piecemeal verification.
