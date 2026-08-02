//! Every document size, and the signal each mounted gallery presents under.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::gallery::probe::ProbeProps;
use crate::section::{
    ArtworkProps, AtomsProps, ChoicesProps, DataProps, DisclosureProps, FeedbackProps, FieldsProps,
    MenusProps, NavigationProps, OverlaysProps, StyledTextProps, SurfacesProps, SvgProps,
};

/// Every document size this measures, and how many copies of the section list each one holds.
///
/// `s0` is the shell alone: the masthead, the probe row and an empty grid. It is the constant of
/// the fit — whatever a frame costs there is what a frame costs before any of the gallery exists.
pub(crate) const SIZES: [&str; 7] = ["s0", "s1", "s2", "s4", "s8", "s13", "s26"];

/// The signal one mounted gallery presents under, which is what a script flips its theme with.
pub(crate) type Scheme = RwSignal<ColorScheme, zgui::reactive::LocalStorage>;

thread_local! {
    /// The scheme signals of the galleries mounted on this thread, oldest first.
    ///
    /// The switch that writes one lives in the gallery's masthead, which this harness does not
    /// mount: the document sizes are meant to differ only in how many sections they hold. So the
    /// signal itself is what a script reaches for, which changes the same custom properties the
    /// switch changes without adding a control to the page that only a script would ever press.
    ///
    /// A *list* and not a slot, because the differential mounts two galleries on one thread and a
    /// slot holds only the second. A script that reached for the slot would flip the theme of
    /// whichever window was built last — twice per step, once while each window was being driven —
    /// so one window would never see a theme change at all and the other would see two that
    /// cancel. Both windows then agree about a theme flip that never happened.
    static SCHEMES: std::cell::RefCell<Vec<Scheme>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The signal of the gallery that was mounted most recently.
///
/// Called once per window, immediately after the window that mounted it was opened, so that each
/// window is driven through the signal its own document reads.
///
/// # Panics
///
/// Panics when no gallery has been mounted since the last call, because a script holding a signal
/// no document reads flips nothing and asserts about a window that never moved.
pub(crate) fn mounted_scheme() -> Scheme {
    SCHEMES.with(|held| {
        held.borrow_mut()
            .pop()
            .expect("a gallery was mounted and published its scheme signal")
    })
}

/// The gallery at one of the sizes.
macro_rules! gallery_of {
    ($name:ident, $sections:expr) => {
        #[component]
        pub(crate) fn $name() -> impl IntoView {
            let scheme = RwSignal::new_local(ColorScheme::Light);
            SCHEMES.with(|held| held.borrow_mut().push(scheme));
            view! {
                ThemeProvider(scheme = scheme) {
                    Toaster {
                        column(class = "page") {
                            Probe()
                            box(class = "grid") {{$sections}}
                        }
                    }
                }
            }
        }
    };
}

gallery_of!(Gallery0, ());
gallery_of!(Gallery1, view! { Atoms() });
gallery_of!(Gallery2, view! { Atoms()Fields() });
gallery_of!(Gallery4, view! { Atoms()Fields()Choices()Feedback() });
gallery_of!(
    Gallery8,
    view! { Atoms()Fields()Choices()Feedback()Disclosure()Overlays()Menus()Navigation() }
);
gallery_of!(
    Gallery13,
    view! {
        Atoms()Fields()Choices()Feedback()Disclosure()Overlays()Menus()Navigation()
        Surfaces()Data()StyledText()Svg()Artwork()
    }
);
gallery_of!(
    Gallery26,
    view! {
        Atoms()Fields()Choices()Feedback()Disclosure()Overlays()Menus()Navigation()
        Surfaces()Data()StyledText()Svg()Artwork()
        Atoms()Fields()Choices()Feedback()Disclosure()Overlays()Menus()Navigation()
        Surfaces()Data()StyledText()Svg()Artwork()
    }
);
