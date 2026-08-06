//! The component that puts a theme into a window.

mod context;

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, Store};
use zgui::{component, view};

use crate::provider::context::ThemeCounter;
use crate::scheme::ColorScheme;
use crate::theme::THEME_SHEET;

pub use crate::provider::context::{ThemeContext, Themes, ThemesStoreFields, use_theme};

/// Puts a set of design tokens into a window, and keeps them there.
///
/// The tokens reach the document as a style sheet of custom properties, which is what makes them
/// overridable: an application that wants a different accent writes one ordinary rule in its own
/// sheet and every component follows, with nothing rebuilt and nothing re-exported.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui_tokens::prelude::*;
///
/// #[component]
/// fn Page() -> impl IntoView {
///     view! {
///         ThemeProvider(scheme = ColorScheme::System) {
///             column(class = "page") {"themed"}
///         }
///     }
/// }
/// ```
///
/// # Which surface the tokens land on
///
/// The outermost provider in a window declares its tokens on `:root`, so everything in the window
/// inherits them — including anything portalled onto an overlay band, which is not a descendant of
/// this component and would otherwise be left untokened. A provider *inside* another one declares
/// on a class of its own instead and themes only its own subtree.
///
/// # Changing the theme
///
/// The tokens live in a store. Applying a whole new theme through
/// [`patch`](zgui::reactive::Patch::patch) wakes only the groups that actually changed, and the
/// document sees one sheet replacement — the sheet keeps its place in the cascade, so what beats
/// what does not silently change with the colours.
///
/// # Light, dark, and the desktop
///
/// [`ColorScheme::System`] is not resolved here. Both token sets are written out and the dark one
/// is put behind `@media (prefers-color-scheme: dark)`, so the answer comes from the same media
/// query the rest of the document is matched against and follows the desktop's own setting with
/// nothing to keep in step.
#[component]
pub fn ThemeProvider(
    /// The tokens in force on a light surface. The framework's own when this is left out.
    ///
    /// A value or a signal. Handing it a signal is what makes the slot *switchable*: writing a
    /// different theme into it re-declares the sheet and the whole window follows, with nothing
    /// remounted and no component told.
    #[prop(into, default = Signal::stored_local(crate::Theme::light()))]
    light: Signal<crate::Theme, zgui::reactive::LocalStorage>,
    /// The tokens in force on a dark surface. The framework's own when this is left out.
    ///
    /// Switchable in the same way, and independently: an interface can be one theme on a light
    /// desktop and another on a dark one.
    #[prop(into, default = Signal::stored_local(crate::Theme::dark()))]
    dark: Signal<crate::Theme, zgui::reactive::LocalStorage>,
    /// Which scheme to present in. Defaults to whichever the desktop asked for.
    #[prop(into, default = Signal::stored_local(ColorScheme::System))]
    scheme: Signal<ColorScheme, zgui::reactive::LocalStorage>,
    /// What the tokens apply to.
    children: Children,
) -> impl IntoView {
    let counter = use_local_context::<ThemeCounter>().unwrap_or_else(|| {
        let counter = ThemeCounter::new();
        provide_local_context(counter.clone());
        counter
    });
    let ordinal = counter.take();

    // The first provider in a window owns `:root`, so that overlay content — which is not below
    // this component in the tree — is themed too. Anything nested themes its own subtree instead,
    // which is what makes a differently-themed panel expressible at all.
    let class = format!("zui-theme-{ordinal}");
    let selector: Rc<str> = if ordinal == 0 {
        Rc::from(":root")
    } else {
        Rc::from(format!(".{class}").as_str())
    };
    let sheet_name: Rc<str> = if ordinal == 0 {
        Rc::from(THEME_SHEET)
    } else {
        Rc::from(format!("{THEME_SHEET}-{ordinal}").as_str())
    };

    let themes = Store::new(Themes {
        light: light.get_untracked(),
        dark: dark.get_untracked(),
    });
    provide_local_context(ThemeContext::new(themes, scheme, Rc::clone(&selector)));

    // The two slots are props and the store is what everything below reads, so one has to follow
    // the other. This way round rather than the reverse: a caller that hands in a signal owns what
    // is in its slot, and the store is this provider's published copy of it. Patching wakes only
    // the token groups that actually differ, so swapping a theme for one that shares its motion
    // does not wake anything reading the motion.
    let following = RenderEffect::new(move |previous: Option<()>| {
        let next = Themes {
            light: light.get(),
            dark: dark.get(),
        };
        // The store was seeded from these above; patching on the first run would be a second write
        // of what it already holds.
        if previous.is_some() {
            use zgui::reactive::Patch as _;
            themes.patch(next);
        }
    });

    // The sheet is a guard rather than a name, because its content is state: it stops being true
    // when this provider goes away, and the guard holds the engine it has to be removed from
    // rather than looking one up on the way out — which a cleanup cannot do.
    let sheet = Stylesheet::install(
        sheet_name.as_ref(),
        &crate::theme::theme_sheet(
            &selector,
            &themes.light().get_untracked(),
            &themes.dark().get_untracked(),
            scheme.get_untracked(),
        ),
    );

    // One effect over the whole theme. It re-runs when a token group changes and when the scheme
    // does, and replacing with text the sheet already has costs nothing — so a patch that happens
    // to change no token reaches the document not at all.
    let installed = {
        let selector = Rc::clone(&selector);
        RenderEffect::new(move |previous: Option<()>| {
            let css = crate::theme::theme_sheet(
                &selector,
                &themes.light().get(),
                &themes.dark().get(),
                scheme.get(),
            );
            // The first run is the install above, already done; running it again would be a second
            // identical write and a second transcript line for one mount.
            if previous.is_some()
                && let Some(sheet) = sheet.as_ref()
            {
                sheet.replace(&css);
            }
        })
    };
    // All three live exactly as long as this component: the cleanup owns both effects' handles and
    // the sheet's guard, and the sheet goes when the provider does.
    on_cleanup_local(move || {
        drop(following);
        drop(installed);
    });

    // `display: contents` because a theme is not a box. The element exists so that a nested
    // provider has something to declare its tokens on, and so that a style sheet can find a themed
    // region; it takes no part in layout.
    view! {
        box(class = class.as_str(), style:display = "contents") {
            {children.into_view_once()}
        }
    }
}
