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

/// Theme status colour (§3.5 — never hard-code green/orange/blue; light /
/// dark / high-contrast all work through the theme). Read from the active
/// theme at render time (`cosmic::theme::active()`), same as libcosmic's
/// own widgets (e.g. `toaster`).
///
/// NOT YET RENDERED: coloured `Text` is unavailable in this iced rev
/// (`<Theme as Catalog>::Class: From<StyleFn>` unsatisfied for both `Text`
/// and `SelectableText` — verified against the vendored source). Views use
/// the default-colour ● until the bound lifts; first render lands with the
/// row that needs it (T9 terminal banner or T11 activity states).
#[allow(dead_code)]
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
        // No hard-coded green/orange/blue (§3.5): Up maps to the theme's
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
    fn status_labels_match_flutter_copy() {
        assert_eq!(status_label(&Status::Up("".into())), "Running");
        assert_eq!(status_label(&Status::Up("2h".into())), "Running 2h");
        assert_eq!(status_label(&Status::Created("".into())), "Created");
        assert_eq!(status_label(&Status::Other("".into())), "Unknown");
        assert_eq!(status_label(&Status::Other("weird".into())), "weird");
    }
}
