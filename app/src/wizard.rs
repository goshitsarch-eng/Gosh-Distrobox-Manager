//! Create wizard + Images page (T7, ux.md §6.7–§6.8, rows #78–#105).
//!
//! Wizard (rows #78–#96): 3-step indicator (dots — no stock stepper, row
//! #78), image grid + selection + check (#79), live search (#80), custom URL
//! (#81), Cancel/Next + inline "Please select an image" (#82), preview card
//! (#83), name + auto-default (#84), init/nvidia toggles (#85–#86), advanced
//! section (#87), home dir field (#88 — plain text input; the folder-suffix
//! icon was dead in Flutter and the portal exposes no directory picker per
//! D16, so no picker is wired), volumes list + add/remove (#89), add-volume
//! dialog with LOUD validation (#90 — the silent return is fixed), Back /
//! Create + `CreateArgName` validation surfaced inline (#91/#96), progress
//! indicator (#92), live console (#93 — shared task mirror, coloured
//! severity, empty state), Cancel/Done/Close (#94), `preselectedImage`
//! wired from Images + dashboard (#95 — dead in Flutter).
//!
//! Images page (rows #97–#105): headline, search, loading/error+Retry/empty,
//! 2-col grid cards (icon, name, tag, + button), details dialog (Tag, URL,
//! Close, Create Container → wizard with preselected image — #102 dead in
//! Flutter), custom URL + arrow → wizard (#103 dead in Flutter). The page is
//! the distro CATALOGUE (`distrobox create --compatibility`), not local
//! images (row #104 — headline says so); local management (#105) is P1
//! backend work, out of scope.

use crate::fl;
use crate::icons::distro_icon;
use crate::message::Message;
use cosmic::widget;

/// Wizard step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WizardStep {
    #[default]
    Image,
    Config,
    Progress,
}

/// Create-wizard state. Lives on `App` (tab state survives nav switches —
/// row #7 comes free because state is on `App`, not the page).
#[derive(Clone, Debug, Default)]
pub struct WizardState {
    pub step: WizardStep,
    pub selected_image: Option<String>,
    pub custom_image: String,
    pub search: String,
    pub name: String,
    pub init_system: bool,
    pub nvidia: bool,
    pub advanced_open: bool,
    pub home_dir: String,
    pub volumes: Vec<WizardVolume>,
    pub volume_dialog: Option<VolumeDialog>,
    pub inline_error: Option<String>,
    /// Spawned create task (progress step reads the T5 mirror by id).
    pub task_id: Option<gosh_distrobox_core::TaskId>,
    /// Latched completion (O2): set when `Completed` arrives for `task_id`,
    /// so the progress step survives the 600 s TTL sweep evicting the mirror.
    pub task_completed: bool,
    pub task_success: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WizardVolume {
    pub host: String,
    pub container: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, Default)]
pub struct VolumeDialog {
    pub host: String,
    pub container: String,
    pub read_only: bool,
    /// LOUD validation error (row #90 — the Flutter dialog silently
    /// returned on bad input).
    pub error: Option<String>,
}

impl WizardState {
    /// Effective image: custom URL wins (Flutter `_getEffectiveImage`).
    pub fn effective_image(&self) -> String {
        if !self.custom_image.trim().is_empty() {
            return self.custom_image.trim().to_string();
        }
        self.selected_image.clone().unwrap_or_default()
    }

    /// Friendly name from a URL (`registry/x:tag` → `X`), Flutter
    /// `_extractImageName` (empty → `container`).
    pub fn image_display_name(image: &str) -> String {
        let last = image.split('/').next_back().unwrap_or("");
        let base = last.split(':').next().unwrap_or("");
        if base.is_empty() {
            return "container".to_string();
        }
        let mut chars = base.chars();
        match chars.next() {
            None => "container".to_string(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }

    /// Tag from a URL (`registry/x:39` → `39`, else `latest`), Flutter
    /// `_extractImageTag`.
    pub fn image_tag(image: &str) -> String {
        let parts: Vec<&str> = image.split(':').collect();
        if parts.len() > 1 {
            parts.last().unwrap_or(&"latest").to_string()
        } else {
            "latest".to_string()
        }
    }

    /// Auto-default name (`{image}-container`, lowercase), Flutter
    /// `_goToConfig` (only fills when the name field is empty).
    pub fn default_name(image: &str) -> String {
        format!(
            "{}-container",
            Self::image_display_name(image).to_lowercase()
        )
    }

    /// Filter images by live query (Flutter `_filterImages`).
    pub fn filter_images<'a>(images: &'a [String], query: &str) -> Vec<&'a String> {
        if query.is_empty() {
            return images.iter().collect();
        }
        let q = query.to_lowercase();
        images
            .iter()
            .filter(|i| i.to_lowercase().contains(&q))
            .collect()
    }

    /// Seed from a preselected image (row #95 — images page, dashboard).
    /// Mirrors Flutter `initState`: sets selection AND the custom field.
    pub fn with_preselected(image: String) -> Self {
        Self {
            selected_image: Some(image.clone()),
            custom_image: image,
            ..Self::default()
        }
    }
}

/// 3-step indicator (row #78): dots, active = wide, completed = filled.
pub fn step_indicator(current: WizardStep) -> cosmic::Element<'static, Message> {
    let order = [WizardStep::Image, WizardStep::Config, WizardStep::Progress];
    let current_idx = order.iter().position(|s| *s == current).unwrap_or(0);
    let mut row = widget::Row::new().spacing(8);
    for (i, _) in order.iter().enumerate() {
        let label = if i < current_idx {
            "●"
        } else if i == current_idx {
            "⬤"
        } else {
            "○"
        };
        row = row.push(widget::text::body(label));
    }
    row.into()
}

/// Image grid card body (rows #79/#101): icon, name, tag. Shared by the
/// wizard grid (select-only — Flutter's card is a whole-card `InkWell` with
/// no Details button) and the images page (select + details). Push widgets
/// directly (T6 pattern): pre-wrapping breaks Theme/Renderer inference in
/// this iced rev.
fn image_card_body(image: &str) -> cosmic::Element<'static, Message> {
    let name = WizardState::image_display_name(image);
    let tag = WizardState::image_tag(image);
    let check: cosmic::Element<'static, Message> = widget::icon::from_name(distro_icon(image))
        .size(32)
        .icon()
        .into();
    widget::Column::new()
        .push(check)
        .push(widget::text::body(name))
        .push(widget::text::caption(tag))
        .spacing(8)
        .align_x(cosmic::iced::Alignment::Center)
        .into()
}

/// Wizard grid card (row #79): body + Select only. NO Details button —
/// Flutter's card has none, and routing details through `ImageSelected`
/// would wipe a typed custom URL (it clears the field).
pub fn wizard_card(
    image: &str,
    selected: bool,
    on_select: Message,
) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(image_card_body(image))
        .push(
            widget::button::standard(if selected {
                fl!("wizard-image-card-selected")
            } else {
                fl!("wizard-image-card-select")
            })
            .on_press(on_select),
        )
        .spacing(8)
        .align_x(cosmic::iced::Alignment::Center)
        .into()
}

/// Images-page grid card (row #101): body + Select→wizard + Details dialog.
pub fn image_card(
    image: &str,
    selected: bool,
    on_select: Message,
    on_details: Message,
) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(image_card_body(image))
        .push(
            widget::button::standard(if selected {
                fl!("wizard-image-card-selected")
            } else {
                fl!("wizard-image-card-select")
            })
            .on_press(on_select),
        )
        .push(widget::button::text(fl!("wizard-image-details")).on_press(on_details))
        .spacing(8)
        .align_x(cosmic::iced::Alignment::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_image_prefers_custom() {
        let mut w = WizardState::default();
        assert_eq!(w.effective_image(), "");
        w.selected_image = Some("a".into());
        assert_eq!(w.effective_image(), "a");
        w.custom_image = "  b  ".into();
        assert_eq!(w.effective_image(), "b");
    }

    #[test]
    fn display_name_and_tag_match_flutter() {
        assert_eq!(
            WizardState::image_display_name("registry.fedoraproject.org/fedora:39"),
            "Fedora"
        );
        assert_eq!(WizardState::image_display_name(""), "container");
        assert_eq!(
            WizardState::image_tag("registry.fedoraproject.org/fedora:39"),
            "39"
        );
        assert_eq!(WizardState::image_tag("ubuntu"), "latest");
        assert_eq!(
            WizardState::default_name("docker.io/library/ubuntu:latest"),
            "ubuntu-container"
        );
    }

    #[test]
    fn filter_is_case_insensitive_live() {
        let images = vec!["Ubuntu:latest".to_string(), "Fedora:41".to_string()];
        assert_eq!(WizardState::filter_images(&images, "").len(), 2);
        assert_eq!(WizardState::filter_images(&images, "UBU").len(), 1);
        assert!(WizardState::filter_images(&images, "zzz").is_empty());
    }

    #[test]
    fn preselected_seeds_both_fields() {
        let w = WizardState::with_preselected("x".into());
        assert_eq!(w.selected_image.as_deref(), Some("x"));
        assert_eq!(w.custom_image, "x");
        assert_eq!(w.effective_image(), "x");
    }
}
