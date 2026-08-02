//! How large an icon is drawn.

use zgui::variants;

variants! {
    /// The axes an [`Icon`](crate::Icon) varies along.
    ///
    /// The sizes are a scale rather than lengths: what each one measures is a token
    /// (`--zui-icon-sm` and its siblings), so an application resizes every icon in an interface by
    /// writing one declaration.
    ///
    /// ```
    /// use zgui_ui_icons::{IconSize, IconVariants};
    ///
    /// let large = IconVariants { size: IconSize::Lg };
    /// assert_eq!(large.class_list(), "zui-icon zui-icon--lg");
    /// assert_eq!(large.data_attributes(), [("data-size", "lg")]);
    /// ```
    pub IconVariants {
        base: "zui-icon",
        size: {
            Sm => "zui-icon--sm",
            Md => "",
            Lg => "zui-icon--lg",
            Xl => "zui-icon--xl",
        } = Md,
    }
}
