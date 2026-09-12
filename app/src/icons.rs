//! Shared distro + status mappings (ux.md §3.6, T6).
//!
//! `_getDistroIcon` was duplicated in 9 Flutter files with three divergent
//! variants (8-outcome, 10-outcome superset with gentoo+void, 6-outcome
//! missing centos/rocky/opensuse/suse); status colour/label in 4 files (2
//! named helpers + 2 inline copies). One table each here — the 10-outcome
//! superset, so the 6-outcome pages inheriting the full table is a behaviour
//! fix, not just a refactor.
//!
//! COSMIC uses named icon-theme lookups; the names below must exist in the
//! theme (pop-icon-theme via the BaseApp, packaging.md §1.1). `distro_icon`
//! returns the theme name; unknown distros fall back to the generic
//! container icon rather than panicking.

use gosh_distrobox_core::models::Status;

/// Theme icon name for a container image reference (matched lowercase, as
/// the Flutter helpers did).
pub fn distro_icon(image: &str) -> &'static str {
    let lower = image.to_lowercase();
    // 10-outcome superset (images_page variant + gentoo/void).
    if lower.contains("ubuntu") {
        "ubuntu"
    } else if lower.contains("fedora") {
        "fedora"
    } else if lower.contains("arch") {
        "archlinux"
    } else if lower.contains("debian") {
        "debian"
    } else if lower.contains("alpine") {
        "alpine"
    } else if lower.contains("centos") || lower.contains("rocky") {
        "centos"
    } else if lower.contains("opensuse") || lower.contains("suse") {
        "opensuse"
    } else if lower.contains("gentoo") {
        "gentoo"
    } else if lower.contains("void") {
        "void"
    } else {
        "container-symbolic"
    }
}

/// Human status label, preserving the Flutter copy exactly
/// (`Running <detail>` / `Created <detail>` / `Exited <detail>` / raw other).
pub fn status_label(status: &Status) -> String {
    match status {
        Status::Up(s) => {
            if s.is_empty() {
                "Running".to_string()
            } else {
                format!("Running {s}")
            }
        }
        Status::Created(s) => {
            if s.is_empty() {
                "Created".to_string()
            } else {
                format!("Created {s}")
            }
        }
        Status::Exited(s) => {
            if s.is_empty() {
                "Exited".to_string()
            } else {
                format!("Exited {s}")
            }
        }
        Status::Other(s) => {
            if s.is_empty() {
                "Unknown".to_string()
            } else {
                s.clone()
            }
        }
    }
}

/// Whether the container is running (Flutter `status is Status_Up`).
pub fn is_running(status: &Status) -> bool {
    matches!(status, Status::Up(_))
}

/// Theme status colour (ux.md §3.5 — never hard-code green/orange/blue; light /
/// dark / high-contrast all work through the theme). Read from the active
/// theme at render time (`cosmic::theme::active()`), same as libcosmic's
/// own widgets (e.g. `toaster`).
///
/// Rendered via `Text::class`, NOT `Text::color`: `color` needs
/// `Theme::Class: From<StyleFn>`, which cosmic's `Text` class cannot satisfy
/// (it is `Copy`, so it cannot hold the boxed closure) — but the class has
/// `From<Color>`, so `.class(status_color(..))` compiles and themes. An
/// earlier comment claimed coloured text was structurally unavailable; T18
/// re-checked the vendored source and found the `class` path.
pub fn status_color(status: &Status) -> cosmic::iced::Color {
    let theme = cosmic::theme::active();
    let cosmic = theme.cosmic();
    match status {
        Status::Up(_) => cosmic.success_color().into(),
        Status::Created(_) => cosmic.accent_color().into(),
        Status::Exited(_) => cosmic.warning_color().into(),
        Status::Other(_) => cosmic.control_7().into(),
    }
}

/// Theme distro colour (ux.md §3.6, row #185 — the half that was never
/// written). Same 10-outcome superset conditions as `distro_icon`, mapped
/// onto theme roles instead of Flutter's hard-coded brand literals (D10:
/// literals defeat high-contrast and user accents, so they are not ported).
///
/// The mapping is deliberately decorative, not semantic: three roles cannot
/// carry ten distro identities, so families rotate over
/// accent/success/warning with unknown falling back to the neutral control —
/// the same fallback shape `distro_icon` uses. `destructive` is excluded on
/// purpose: no decorative marker should borrow the destructive semantic.
/// Distro identity is always carried by the icon + name as well, never by
/// colour alone.
pub fn distro_colour(image: &str) -> cosmic::iced::Color {
    let theme = cosmic::theme::active();
    let cosmic = theme.cosmic();
    let lower = image.to_lowercase();
    // Same branch order as `distro_icon` (the 10-outcome superset); the role
    // rotation keeps adjacent families distinct.
    if lower.contains("ubuntu") {
        cosmic.accent_color().into()
    } else if lower.contains("fedora") {
        cosmic.success_color().into()
    } else if lower.contains("arch") {
        cosmic.warning_color().into()
    } else if lower.contains("debian") {
        cosmic.accent_color().into()
    } else if lower.contains("alpine") {
        cosmic.success_color().into()
    } else if lower.contains("centos") || lower.contains("rocky") {
        cosmic.warning_color().into()
    } else if lower.contains("opensuse") || lower.contains("suse") {
        cosmic.accent_color().into()
    } else if lower.contains("gentoo") {
        cosmic.success_color().into()
    } else if lower.contains("void") {
        cosmic.warning_color().into()
    } else {
        cosmic.control_7().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distro_table_covers_superset() {
        assert_eq!(distro_icon("docker.io/library/ubuntu:latest"), "ubuntu");
        assert_eq!(distro_icon("Fedora 41"), "fedora");
        assert_eq!(distro_icon("archlinux:latest"), "archlinux");
        assert_eq!(distro_icon("debian:12"), "debian");
        assert_eq!(distro_icon("alpine:edge"), "alpine");
        assert_eq!(distro_icon("rockylinux:9"), "centos");
        assert_eq!(distro_icon("opensuse/tumbleweed"), "opensuse");
        assert_eq!(distro_icon("gentoo/stage3"), "gentoo");
        assert_eq!(distro_icon("voidlinux"), "void");
        assert_eq!(distro_icon("something-else"), "container-symbolic");
    }

    #[test]
    fn status_colors_follow_theme_roles() {
        // No hard-coded green/orange/blue (ux.md §3.5): Up maps to the theme's
        // success role, Created to accent, Exited to warning, Other to a
        // neutral control colour. Assert against the live theme (not fixed
        // RGB) so light/dark/high-contrast all satisfy this by construction.
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();
        assert_eq!(
            status_color(&Status::Up("".into())),
            cosmic.success_color().into()
        );
        assert_eq!(
            status_color(&Status::Created("".into())),
            cosmic.accent_color().into()
        );
        assert_eq!(
            status_color(&Status::Exited("".into())),
            cosmic.warning_color().into()
        );
        assert_eq!(
            status_color(&Status::Other("".into())),
            cosmic.control_7().into()
        );
    }

    #[test]
    fn distro_colours_follow_theme_roles() {
        // Row #185: the same 10-outcome superset as `distro_icon`, pinned
        // against the live theme (not fixed RGB) so light/dark/high-contrast
        // all satisfy this by construction. Rotation order and the neutral
        // fallback are load-bearing — a reorder silently re-tints families.
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();
        let (accent, success, warning, neutral) = (
            cosmic.accent_color(),
            cosmic.success_color(),
            cosmic.warning_color(),
            cosmic.control_7(),
        );
        for image in ["ubuntu:24.04", "Debian 12", "opensuse/tumbleweed"] {
            assert_eq!(
                distro_colour(image),
                accent.into(),
                "{image} rides the accent role"
            );
        }
        for image in ["Fedora 41", "alpine:edge", "gentoo/stage3"] {
            assert_eq!(
                distro_colour(image),
                success.into(),
                "{image} rides the success role"
            );
        }
        for image in ["archlinux:latest", "rockylinux:9", "voidlinux"] {
            assert_eq!(
                distro_colour(image),
                warning.into(),
                "{image} rides the warning role"
            );
        }
        assert_eq!(
            distro_colour("something-else"),
            neutral.into(),
            "unknown distros fall back to the neutral control, like `distro_icon`"
        );
    }

    #[test]
    fn status_labels_match_flutter_copy() {
        assert_eq!(status_label(&Status::Up("".into())), "Running");
        assert_eq!(status_label(&Status::Up("2h".into())), "Running 2h");
        assert_eq!(status_label(&Status::Created("".into())), "Created");
        assert_eq!(status_label(&Status::Other("".into())), "Unknown");
        assert_eq!(status_label(&Status::Other("weird".into())), "weird");
    }
}
