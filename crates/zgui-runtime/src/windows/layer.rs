//! What a window that is part of the desktop shell is asked for before it exists.

use zgui_geom::{Css, CssPx, Edges, Size};
use zgui_platform::{
    Decorations, ExclusiveZone, KeyboardInteractivity, Layer, LayerPlacement, MonitorTarget,
    SurfaceAttributes, SurfaceRole,
};

/// Where a shell window sits, how much of the screen it claims, and how large it is.
///
/// A shell window is a bar, a dock, a notification, a pop-over or a wallpaper: a part of the
/// desktop itself. The desktop draws no frame around it, and the user
/// cannot move or resize it. Only a desktop that offers shell windows opens one; see
/// [`PlatformCapabilities::layer_surfaces`](zgui_platform::PlatformCapabilities::layer_surfaces).
///
/// ```
/// use zgui_geom::Edges;
/// use zgui_platform::{ExclusiveZone, KeyboardInteractivity};
/// use zgui_runtime::windows::{LayerOptions, WindowOptions};
///
/// // A bar across the top of the screen that keeps windows out of its 34 pixels.
/// let bar = WindowOptions::layer(
///     LayerOptions::top()
///         .with_anchors(Edges::new(true, true, false, true))
///         .with_size(0.0, 34.0)
///         .with_exclusive_zone(ExclusiveZone::Reserve(zgui_geom::CssPx(34.0)))
///         .with_namespace("bar"),
/// );
/// assert!(matches!(
///     bar.attributes().role,
///     zgui_platform::SurfaceRole::Layer(_)
/// ));
///
/// // A pop-over below it that takes the keyboard while the user works in it.
/// let popover = LayerOptions::top()
///     .with_anchors(Edges::new(true, false, false, true))
///     .with_margin(8.0, 0.0, 0.0, 8.0)
///     .with_keyboard(KeyboardInteractivity::OnDemand)
///     .with_size(320.0, 240.0);
/// # let _ = popover;
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerOptions {
    /// Where the window sits and what it claims.
    placement: LayerPlacement,
    /// The size the window starts at, in CSS pixels.
    size: Option<Size<CssPx, Css>>,
}

impl LayerOptions {
    /// A shell window on `layer`, anchored to no edge, that stays out of the room others reserve.
    pub fn new(layer: Layer) -> Self {
        Self {
            placement: LayerPlacement::new(layer),
            size: None,
        }
    }

    /// A shell window behind everything, such as a wallpaper.
    pub fn background() -> Self {
        Self::new(Layer::Background)
    }

    /// A shell window below ordinary windows, such as a desktop widget.
    pub fn bottom() -> Self {
        Self::new(Layer::Bottom)
    }

    /// A shell window above ordinary windows, such as a bar or a dock.
    pub fn top() -> Self {
        Self::new(Layer::Top)
    }

    /// A shell window above everything, such as a notification or a launcher.
    pub fn overlay() -> Self {
        Self::new(Layer::Overlay)
    }

    /// Fastens the window to the screen edges that `anchors` names.
    ///
    /// Two opposite edges stretch the window between them. No edges centres it.
    pub fn with_anchors(mut self, anchors: Edges<bool>) -> Self {
        self.placement.anchors = anchors;
        self
    }

    /// Sets the distance from each anchored edge, in CSS pixels.
    pub fn with_margin(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.placement.margin = Edges::new(CssPx(top), CssPx(right), CssPx(bottom), CssPx(left));
        self
    }

    /// Sets how the window and the room other shell windows reserve affect each other.
    pub fn with_exclusive_zone(mut self, zone: ExclusiveZone) -> Self {
        self.placement.exclusive_zone = zone;
        self
    }

    /// Sets whether the window takes the keyboard.
    pub fn with_keyboard(mut self, keyboard: KeyboardInteractivity) -> Self {
        self.placement.keyboard = keyboard;
        self
    }

    /// Names what the window is for, in a word the desktop's rules can match.
    ///
    /// Without one the desktop sees the application identifier.
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.placement.namespace = Some(namespace.into());
        self
    }

    /// Puts the window on `monitor`: a [`MonitorId`](zgui_platform::MonitorId) from
    /// [`use_monitors`](crate::monitors::use_monitors), or an output name. Without one the
    /// desktop chooses.
    ///
    /// The window does not open when no connected output matches. It closes with
    /// [`CloseReason::OutputGone`](zgui_platform::CloseReason::OutputGone) when its output goes
    /// away.
    pub fn with_monitor(mut self, monitor: impl Into<MonitorTarget>) -> Self {
        self.placement.monitor = Some(monitor.into());
        self
    }

    /// Sets the size the window starts at, in CSS pixels.
    ///
    /// The size along an axis fastened to both of its edges is ignored, so a bar across the top
    /// gives only its height and writes zero for its width.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = Some(Size::new(CssPx(width), CssPx(height)));
        self
    }

    /// Where the window sits and what it claims.
    pub fn placement(&self) -> &LayerPlacement {
        &self.placement
    }

    /// The size the window starts at, if one was set.
    pub fn size(&self) -> Option<Size<CssPx, Css>> {
        self.size
    }

    /// Makes `attributes` describe this shell window, keeping its title, its identifier and its
    /// transparency.
    pub(crate) fn apply_to(self, attributes: &mut SurfaceAttributes) {
        attributes.role = SurfaceRole::Layer(self.placement);
        attributes.resizable = false;
        attributes.decorations = Decorations::None;
        if self.size.is_some() {
            attributes.size = self.size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LayerOptions;
    use zgui_geom::{CssPx, Edges, Size};
    use zgui_platform::{
        Decorations, ExclusiveZone, KeyboardInteractivity, Layer, MonitorId, MonitorTarget,
        SurfaceAttributes, SurfaceRole,
    };

    #[test]
    fn a_shell_window_opens_on_an_output_named_by_its_identifier() {
        let options = LayerOptions::top().with_monitor(MonitorId::new(9));
        assert_eq!(
            options.placement().monitor,
            Some(MonitorTarget::Id(MonitorId::new(9)))
        );
    }

    #[test]
    fn every_option_reaches_the_placement() {
        let options = LayerOptions::overlay()
            .with_anchors(Edges::new(true, false, false, true))
            .with_margin(1.0, 2.0, 3.0, 4.0)
            .with_exclusive_zone(ExclusiveZone::Ignore)
            .with_keyboard(KeyboardInteractivity::OnDemand)
            .with_namespace("popover")
            .with_monitor("DP-1")
            .with_size(320.0, 200.0);
        let placement = options.placement();
        assert_eq!(placement.layer, Layer::Overlay);
        assert_eq!(placement.anchors, Edges::new(true, false, false, true));
        assert_eq!(
            placement.margin,
            Edges::new(CssPx(1.0), CssPx(2.0), CssPx(3.0), CssPx(4.0))
        );
        assert_eq!(placement.exclusive_zone, ExclusiveZone::Ignore);
        assert_eq!(placement.keyboard, KeyboardInteractivity::OnDemand);
        assert_eq!(placement.namespace.as_deref(), Some("popover"));
        assert_eq!(placement.monitor, Some(MonitorTarget::from("DP-1")));
        assert_eq!(options.size(), Some(Size::new(CssPx(320.0), CssPx(200.0))));
    }

    #[test]
    fn a_shell_window_is_undecorated_and_keeps_what_it_did_not_name() {
        let mut attributes = SurfaceAttributes::new("bar");
        attributes.transparent = true;
        attributes.size = Some(Size::new(CssPx(10.0), CssPx(10.0)));
        LayerOptions::top().apply_to(&mut attributes);
        assert!(matches!(attributes.role, SurfaceRole::Layer(_)));
        assert_eq!(attributes.decorations, Decorations::None);
        assert!(!attributes.resizable);
        assert!(attributes.transparent);
        assert_eq!(attributes.title.as_str(), "bar");
        assert_eq!(
            attributes.size,
            Some(Size::new(CssPx(10.0), CssPx(10.0))),
            "a size the options did not name is kept"
        );
    }
}
