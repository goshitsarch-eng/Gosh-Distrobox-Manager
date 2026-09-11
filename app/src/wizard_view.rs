//! Wizard step views (T7, rows #78–#94).
//!
//! Step 0 (image): search + grid + custom URL + Cancel/Next with INLINE
//! "Please select an image" (#82 — not a snackbar). Step 1 (config):
//! preview card (#83), name + auto-default (#84), init/nvidia toggles
//! (#85–#86, `settings::item toggler`), advanced section (#87), home dir
//! field (#88, plain input — D16 re-scope), volumes + add/remove (#89),
//! Back/Create + inline `CreateArgName` error (#91/#96). Step 2 (progress):
//! determinate indicator (#92), live console from the T5 mirror with
//! severity colouring + empty state (#93 — auto-scroll deliberately
//! omitted: no `scroll_to` hook without fighting user scroll, §3.1 note),
//! Cancel/Done/Close (#94). Add-volume dialog (#90) with LOUD errors.

use crate::message::{Message, WizardMsg};
use crate::views::empty_state;
use crate::wizard::{VolumeDialog, WizardState, WizardStep, step_indicator, wizard_card};
use cosmic::iced::Length;
use cosmic::widget;

/// Full wizard body (pushed over Containers like details).
pub fn view_wizard(
    state: &WizardState,
    images: &[String],
    loading_images: bool,
    task_output: Option<&[String]>,
    task_completed: bool,
    task_success: bool,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(12);
    col = col.push(step_indicator(state.step));
    match state.step {
        WizardStep::Image => col = col.push(view_step_image(state, images, loading_images)),
        WizardStep::Config => col = col.push(view_step_config(state)),
        WizardStep::Progress => {
            col = col.push(view_step_progress(
                task_output,
                task_completed,
                task_success,
            ))
        }
    }
    widget::scrollable(col).into()
}

fn view_step_image(
    state: &WizardState,
    images: &[String],
    loading: bool,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(12);
    col = col.push(widget::text::title3("Select Image"));
    col = col.push(widget::text::body(
        "Choose a Linux distribution for your container.",
    ));
    col = col.push({
        let search: cosmic::Element<'static, Message> =
            widget::text_input::search_input("Search distributions...", state.search.clone())
                .on_input(|s| Message::Wizard(WizardMsg::SearchChanged(s)))
                .into();
        search
    });
    if loading && images.is_empty() {
        col = col.push(widget::text::body("Loading images…"));
    } else {
        let filtered = WizardState::filter_images(images, &state.search);
        if filtered.is_empty() {
            col = col.push(empty_state(
                "document-open-symbolic",
                "No images available".to_string(),
                String::new(),
                None,
            ));
        } else {
            let mut grid = widget::grid::grid();
            for (i, image) in filtered.iter().enumerate() {
                let img = (*image).clone();
                grid = grid.push(wizard_card(
                    image,
                    state.selected_image.as_deref() == Some(image.as_str())
                        || (!state.custom_image.is_empty() && state.custom_image == **image),
                    Message::Wizard(WizardMsg::ImageSelected(img)),
                ));
                if i % 2 == 1 {
                    grid = grid.insert_row();
                }
            }
            col = col.push({
                let grid_el: cosmic::Element<'static, Message> = grid.into();
                grid_el
            });
        }
    }
    // Custom URL (#81).
    col = col.push(widget::text::caption_heading("CUSTOM IMAGE URL"));
    col = col.push({
        let input: cosmic::Element<'static, Message> = widget::text_input::text_input(
            "e.g. docker.io/library/ubuntu:22.04",
            state.custom_image.clone(),
        )
        .on_input(|s| Message::Wizard(WizardMsg::CustomChanged(s)))
        .into();
        input
    });
    // Inline error (#82 — not a snackbar).
    if let Some(err) = &state.inline_error {
        col = col.push(widget::warning(err.clone()));
    }
    col = col.push({
        let buttons: cosmic::Element<'static, Message> = widget::Row::new()
            .push(widget::button::standard("Cancel").on_press(Message::Wizard(WizardMsg::Closed)))
            .push(
                widget::button::suggested("Next")
                    .on_press(Message::Wizard(WizardMsg::NextFromImage)),
            )
            .spacing(12)
            .into();
        buttons
    });
    col.into()
}

fn view_step_config(state: &WizardState) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(12);
    col = col.push(widget::text::title3("Configuration"));
    col = col.push(widget::text::caption("Step 2 of 3: System settings"));
    // Preview card (#83).
    col = col.push({
        let preview: cosmic::Element<'static, Message> = widget::Column::new()
            .push(widget::text::caption("SELECTED IMAGE"))
            .push(widget::text::body(state.effective_image()))
            .spacing(4)
            .into();
        preview
    });
    // Name (#84).
    col = col.push(widget::text::body("Container Name"));
    col = col.push({
        let input: cosmic::Element<'static, Message> =
            widget::text_input::text_input("e.g. arch-dev-box", state.name.clone())
                .on_input(|s| Message::Wizard(WizardMsg::NameChanged(s)))
                .into();
        input
    });
    // Toggles (#85–#86). `settings::item toggler` returns a ListButton —
    // it goes through `list_column().add()`, not `Column::push` (T6
    // ListButton lesson: no `From<ListButton> for Element`).
    col = col.push({
        let mut toggles = widget::list_column::list_column();
        toggles = toggles.add(
            widget::settings::item::builder("Init System")
                .description("Run an init system inside the container")
                .toggler(state.init_system, |v| {
                    Message::Wizard(WizardMsg::InitToggled(v))
                }),
        );
        toggles = toggles.add(
            widget::settings::item::builder("NVIDIA GPU Support")
                .description("Enable NVIDIA GPU passthrough")
                .toggler(state.nvidia, |v| {
                    Message::Wizard(WizardMsg::NvidiaToggled(v))
                }),
        );
        let toggles_el: cosmic::Element<'static, Message> = toggles.into_element();
        toggles_el
    });
    // Advanced (#87): collapsed by default (matches Flutter's
    // `ExpansionTile(initiallyExpanded: false)`).
    if state.advanced_open {
        col = col.push(
            widget::button::text("Advanced Options ▾")
                .on_press(Message::Wizard(WizardMsg::AdvancedToggled)),
        );
        // Home dir (#88 — plain input; D16: no directory picker).
        col = col.push(widget::text::body("Home Directory"));
        col = col.push({
            let input: cosmic::Element<'static, Message> = widget::text_input::text_input(
                "/home/user/containers/my-box",
                state.home_dir.clone(),
            )
            .on_input(|s| Message::Wizard(WizardMsg::HomeChanged(s)))
            .into();
            input
        });
        // Volumes (#89).
        col = col.push(widget::text::body("Volume Mounts"));
        if state.volumes.is_empty() {
            col = col.push(widget::text::caption("No volumes configured"));
        } else {
            for (i, v) in state.volumes.iter().enumerate() {
                let ro = if v.read_only { " (Read-only)" } else { "" };
                col = col.push({
                    let row: cosmic::Element<'static, Message> = widget::Row::new()
                        .push(
                            widget::text::body(format!("{} -> {}{}", v.host, v.container, ro))
                                .width(Length::Fill),
                        )
                        .push(
                            widget::button::text("Remove")
                                .on_press(Message::Wizard(WizardMsg::VolumeRemoved(i))),
                        )
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center)
                        .into();
                    row
                });
            }
        }
        col = col.push(
            widget::button::standard("Add Volume")
                .on_press(Message::Wizard(WizardMsg::VolumeAddRequested)),
        );
    } else {
        col = col.push(
            widget::button::text("Advanced Options ▸")
                .on_press(Message::Wizard(WizardMsg::AdvancedToggled)),
        );
    }
    // Inline error (#91/#96 — CreateArgName surfaced, not snackbar).
    if let Some(err) = &state.inline_error {
        col = col.push(widget::warning(err.clone()));
    }
    col = col.push({
        let buttons: cosmic::Element<'static, Message> = widget::Row::new()
            .push(
                widget::button::standard("Back").on_press(Message::Wizard(WizardMsg::BackToImage)),
            )
            .push(
                widget::button::suggested("Create")
                    .on_press(Message::Wizard(WizardMsg::CreateRequested)),
            )
            .spacing(12)
            .into();
        buttons
    });
    col.into()
}

fn view_step_progress(
    task_output: Option<&[String]>,
    task_completed: bool,
    task_success: bool,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(12);
    col = col.push(widget::text::title3(if task_completed {
        if task_success {
            "Container Created!"
        } else {
            "Creation Failed"
        }
    } else {
        "Creating Container..."
    }));
    // Determinate indicator (#92): 1.0 when done, indeterminate while
    // running (progress None pattern mirrors the determinate_circular API:
    // value 0.0–1.0; running shows the spinning form via indeterminate).
    col = col.push({
        let bar: cosmic::Element<'static, Message> = if task_completed {
            widget::progress_bar::determinate_circular(1.0).into()
        } else {
            widget::progress_bar::indeterminate_circular().into()
        };
        bar
    });
    // Console (#93): mirror lines with severity colouring via caption/body
    // choice (coloured Text unavailable — T6 bound), empty state, NO
    // auto-scroll (deliberate: scroll_to would fight user scroll, §3.1).
    match task_output {
        None | Some([]) => {
            col = col.push(widget::text::caption("Waiting for output..."));
        }
        Some(lines) => {
            let mut console = widget::Column::new().spacing(2);
            for line in lines.iter() {
                let lower = line.to_lowercase();
                if lower.contains("error") || lower.contains("failed") {
                    console = console.push(widget::text::body(line.clone()));
                } else {
                    console = console.push(widget::text::caption(line.clone()));
                }
            }
            col = col.push({
                let scroll: cosmic::Element<'static, Message> = widget::scrollable(console)
                    .height(Length::Fixed(300.0))
                    .into();
                scroll
            });
        }
    }
    // Cancel / Done / Close (#94).
    if task_completed {
        col = col.push({
            let buttons: cosmic::Element<'static, Message> = widget::Row::new()
                .push(
                    widget::button::suggested("Done")
                        .on_press(Message::Wizard(WizardMsg::ProgressDone)),
                )
                .spacing(12)
                .into();
            buttons
        });
    } else {
        col = col.push(
            widget::button::standard("Cancel")
                .on_press(Message::Wizard(WizardMsg::ProgressCancelRequested)),
        );
    }
    col.into()
}

/// Add-volume dialog body (#90): host, container, read-only + LOUD error.
pub fn volume_dialog_body(dialog: &VolumeDialog) -> cosmic::Element<'static, Message> {
    use crate::message::WizardMsg;
    let mut col = widget::Column::new().spacing(8);
    col = col.push(widget::text::body("Host Path"));
    col = col.push({
        let input: cosmic::Element<'static, Message> =
            widget::text_input::text_input("/mnt/data", dialog.host.clone())
                .on_input(|s| Message::Wizard(WizardMsg::VolumeDialogHostChanged(s)))
                .into();
        input
    });
    col = col.push(widget::text::body("Container Path"));
    col = col.push({
        let input: cosmic::Element<'static, Message> =
            widget::text_input::text_input("/mnt/data", dialog.container.clone())
                .on_input(|s| Message::Wizard(WizardMsg::VolumeDialogContainerChanged(s)))
                .into();
        input
    });
    col = col.push({
        let mut ro = widget::list_column::list_column();
        ro = ro.add(
            widget::settings::item::builder("Read-only").toggler(dialog.read_only, |v| {
                Message::Wizard(WizardMsg::VolumeDialogReadOnlyToggled(v))
            }),
        );
        let ro_el: cosmic::Element<'static, Message> = ro.into_element();
        ro_el
    });
    if let Some(err) = &dialog.error {
        col = col.push(widget::warning(err.clone()));
    }
    col.into()
}
