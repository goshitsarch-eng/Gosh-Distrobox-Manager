//! Images page view (T7, rows #97–#105).
//!
//! Headline + subtitle (#98, catalogue semantics #104), live search (#99),
//! loading / error+Retry / empty states (#100, shared `empty_state`), 2-col
//! grid cards (icon, name, tag, Select→wizard, Details — #101), custom URL +
//! arrow → wizard (#103). Details dialog (#102) shows Tag + Image URL with
//! Close + Create Container (wires `preselectedImage`, dead in Flutter).

use crate::fl;
use crate::message::{ImageMsg, Message};
use crate::views::empty_state;
use crate::wizard::{WizardState, image_card};
use cosmic::iced::Length;
use cosmic::widget;

/// Images page body. Stateless over `App` fields (T6 pattern — the module
/// owns no state; `App::update` owns the arms).
pub fn view_images(
    images: &[String],
    loading: bool,
    error: Option<String>,
    search: &str,
    custom: &str,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(12);

    // Headline (#98) + catalogue semantics (#104).
    col = col.push(widget::text::title2(fl!("misc-images-title")));
    col = col.push(widget::text::body(fl!("misc-images-subtitle")));

    // Search (#99).
    col = col.push({
        let search_el: cosmic::Element<'static, Message> = widget::text_input::search_input(
            fl!("misc-images-search-placeholder"),
            search.to_string(),
        )
        .id(cosmic::iced::widget::Id::new(crate::views::SEARCH_IMAGES))
        .on_input(|s| Message::Images(ImageMsg::SearchChanged(s)))
        .into();
        search_el
    });

    // Grid states (#100).
    if loading && images.is_empty() {
        col = col.push({
            let spin: cosmic::Element<'static, Message> =
                widget::container(widget::text::body(fl!("misc-images-loading")))
                    .width(Length::Fill)
                    .center_x(Length::Fill)
                    .into();
            spin
        });
    } else if let Some(err) = error.filter(|_| images.is_empty()) {
        col = col.push(empty_state(
            "dialog-error-symbolic",
            fl!("misc-images-could-not-load"),
            err,
            Some(
                widget::button::standard(fl!("action-retry"))
                    .on_press(Message::Images(ImageMsg::LoadRequested))
                    .into(),
            ),
        ));
    } else {
        let filtered = WizardState::filter_images(images, search);
        if filtered.is_empty() {
            col = col.push(empty_state(
                "document-open-symbolic",
                if search.is_empty() {
                    fl!("misc-images-empty")
                } else {
                    fl!("misc-images-empty-match")
                },
                String::new(),
                None,
            ));
        } else {
            // 2-col grid (#101): `widget::grid` with insert_row every 2.
            let mut grid = widget::grid::grid();
            for (i, image) in filtered.iter().enumerate() {
                let img = (*image).clone();
                let img2 = (*image).clone();
                grid = grid.push(image_card(
                    image,
                    false,
                    Message::Images(ImageMsg::CreateWithImage(img)),
                    Message::Images(ImageMsg::DetailsRequested(img2)),
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

    // Custom URL + arrow (#103 → wizard with preselected image).
    col = col.push(widget::text::caption_heading(fl!(
        "misc-images-custom-heading"
    )));
    col = col.push({
        let input: cosmic::Element<'static, Message> = widget::text_input::text_input(
            fl!("misc-images-custom-placeholder"),
            custom.to_string(),
        )
        .on_input(|s| Message::Images(ImageMsg::CustomChanged(s)))
        .into();
        input
    });
    col = col.push(
        widget::button::suggested(fl!("misc-images-custom-submit"))
            .on_press(Message::Images(ImageMsg::CustomSubmitted)),
    );
    col = col.push(widget::text::caption(fl!("misc-images-custom-help")));

    widget::scrollable(col).into()
}

/// Image details dialog (#102): Tag + Image URL + Close + Create Container
/// (preselected — the Flutter TODO wired).
pub fn image_details_dialog(image: &str) -> cosmic::Element<'static, Message> {
    let name = WizardState::image_display_name(image);
    let tag = WizardState::image_tag(image);
    widget::Column::new()
        .push(widget::text::title3(name))
        .push(widget::text::caption(fl!("misc-images-tag", tag = tag)))
        .push(widget::text::body(image.to_string()))
        .spacing(8)
        .into()
}

/// Image details dialog actions (Close + Create Container with image).
pub fn image_details_actions(
    image: String,
) -> (
    cosmic::Element<'static, Message>,
    cosmic::Element<'static, Message>,
) {
    let close: cosmic::Element<'static, Message> = widget::button::standard(fl!("action-close"))
        .on_press(Message::Images(ImageMsg::DetailsClosed))
        .into();
    let create: cosmic::Element<'static, Message> =
        widget::button::suggested(fl!("misc-images-create-container"))
            .on_press(Message::Images(ImageMsg::CreateWithImage(image)))
            .into();
    (close, create)
}
