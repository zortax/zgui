//! The gallery itself: every panel, under one theme provider and one toaster.
//!
//! This is a module rather than the body of `main` so that the same window can be opened by
//! something other than a person — the probe beside it drives this exact component, so what is
//! driven and what is shipped cannot drift apart.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::section::{
    ArtworkProps, AtomsProps, ChoicesProps, CompositesProps, DataProps, DisclosureProps,
    FeedbackProps, FieldsProps, MenusProps, NavigationProps, OverlaysProps, StyledTextProps,
    SurfacesProps, SvgProps,
};
use crate::shell::MastheadProps;

/// How wide the window opens, in CSS pixels.
pub(crate) const WIDTH: f32 = 1600.0;

/// How tall the window opens, in CSS pixels.
pub(crate) const HEIGHT: f32 = 1000.0;

/// The gallery.
#[component]
pub(crate) fn Gallery() -> impl IntoView {
    let scheme = RwSignal::new_local(ColorScheme::Light);

    view! {
        ThemeProvider(scheme = scheme) {
            Toaster {
                column(class = "page") {
                    Masthead(scheme = scheme)
                    box(class = "grid") {
                        Atoms()
                        Fields()
                        Choices()
                        Composites()
                        Feedback()
                        Disclosure()
                        Overlays()
                        Menus()
                        Navigation()
                        Surfaces()
                        Data()
                        StyledText()
                        Svg()
                        Artwork()
                    }
                }
            }
        }
    }
}
