//! The macros that put a declaration next to the code it is about.
//!
//! A declaration is written in the module that reads the property, and never in a central table.
//! That is the whole mechanism: a table maintained by hand goes stale the first time a reader is
//! renamed, split or deleted, whereas a declaration that lives in the same file cannot go stale
//! without someone editing the file it is stale in.

/// Declares how the module it appears in treats one CSS longhand.
///
/// The longhand is named by its Rust spelling — underscores where a style sheet writes hyphens.
/// The declaration expands to a hidden constant named after the property, so declaring the same
/// property twice in one module is a compile error rather than two disagreeing answers.
///
/// ```
/// use zgui_css::parity::Support;
/// use zgui_css::register_property;
///
/// register_property!(border_top_left_radius => Support::Implemented("zgui-paint::lower::border"));
/// register_property!(border_image_outset    => Support::Ignored("border images are not painted yet"));
///
/// assert_eq!(border_top_left_radius.css_name(), "border-top-left-radius");
/// assert!(border_image_outset.support().is_reachable());
/// ```
#[macro_export]
macro_rules! register_property {
    ($longhand:ident => $support:expr) => {
        #[doc(hidden)]
        #[allow(non_upper_case_globals, unreachable_pub, dead_code)]
        pub const $longhand: $crate::parity::Registration =
            $crate::parity::Registration::new(::core::stringify!($longhand), $support);
    };
}

/// Declares a group of longhands and the list of them, in one place.
///
/// Same declaration as [`register_property!`] for each row, plus a `REGISTERED` slice holding
/// exactly those rows. The slice is built out of the constants the same invocation defines, so a
/// row cannot be declared and left out of the list, and the list cannot name a row that was never
/// declared.
///
/// ```
/// use zgui_css::parity::{AbsentReason, Registry, Support};
/// use zgui_css::register_properties;
///
/// mod svg_paint {
///     use zgui_css::parity::{AbsentReason, Support};
///     zgui_css::register_properties! {
///         fill_rule  => Support::Absent(AbsentReason::GeckoOnly),
///         clip_rule  => Support::Absent(AbsentReason::GeckoOnly),
///     }
/// }
///
/// let mut registry = Registry::new();
/// registry.extend(svg_paint::REGISTERED).expect("no row declared twice");
/// assert_eq!(registry.len(), 2);
/// ```
#[macro_export]
macro_rules! register_properties {
    ($( $longhand:ident => $support:expr ),+ $(,)?) => {
        $( $crate::register_property!($longhand => $support); )+

        /// Every longhand declared in this module.
        #[allow(unreachable_pub)]
        pub const REGISTERED: &[$crate::parity::Registration] = &[ $( $longhand ),+ ];
    };
}
