//! The card that names what the pointer is over.

use zgui::prelude::*;
use zgui::view::CustomPropertyName;
use zgui::{component, view};

use crate::chart::scale::tick_label;

/// What the swatch beside a name is drawn as.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum ChartIndicator {
    /// A small square, which is what a bar and a point want.
    #[default]
    Dot,
    /// A narrow upright bar, which reads as a stroke.
    Line,
    /// An outline only, for a series drawn as a dashed stroke.
    Dashed,
}

impl ChartIndicator {
    /// The value written to `data-indicator`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Line => "line",
            Self::Dashed => "dashed",
        }
    }
}

/// One line of a tooltip: a series, its colour and its value at the point under the pointer.
#[derive(Clone, PartialEq, Debug)]
pub struct ChartEntry {
    /// What the series is called.
    pub name: String,
    /// Its value here.
    pub value: f64,
    /// Which colour it is drawn in, as the custom property the sheet paints from.
    pub tone: String,
}

impl ChartEntry {
    /// One line, for a series called `name` drawn in `tone`.
    #[must_use]
    pub fn new(name: impl Into<String>, value: f64, tone: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value,
            tone: tone.into(),
        }
    }
}

/// The card a chart shows for whatever the pointer is over.
///
/// A component of its own rather than something the chart draws inline, because the same card is
/// what a caller wants when they place their own readout beside a chart — and because what is in it
/// is a list of series and values, which is data rather than a picture.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::chart::{ChartEntry, ChartTooltipContentProps};
///
/// /// What February held.
/// #[component]
/// fn February() -> impl IntoView {
///     view! {
///         ChartTooltipContent(
///             label = "February",
///             entries = vec![ChartEntry::new("Units", 180.0, "var(--zui-color-chart-1)")]
///         )
///     }
/// }
/// ```
#[component]
pub fn ChartTooltipContent(
    /// What the point under the pointer is called.
    #[prop(into, optional)]
    label: Option<String>,
    /// The series and their values here.
    entries: Vec<ChartEntry>,
    /// What the swatch beside each name is drawn as.
    #[prop(default = ChartIndicator::Dot)]
    indicator: ChartIndicator,
    /// Whether to leave the point's name off.
    #[prop(default = false)]
    hide_label: bool,
    /// Whether to leave the swatches off.
    #[prop(default = false)]
    hide_indicator: bool,
    /// Classes merged after the card's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    let heading = (!hide_label)
        .then_some(label)
        .flatten()
        .map(|text| view! { text(class = "zui-chart__tooltip-label") {{text}} });

    let rows: Vec<AnyView> = entries
        .into_iter()
        .map(|entry| {
            let swatch = (!hide_indicator).then(|| {
                let tone = entry.tone.clone();
                AnyView::new(view! {
                    box(
                        class = "zui-chart__tooltip-swatch",
                        a11y:hidden = true,
                        {..Attrs::new().custom_property(
                            CustomPropertyName::new("zui-chart-tone"),
                            move || Some(tone.clone()),
                        )}
                    )
                })
            });
            AnyView::new(view! {
                row(class = "zui-chart__tooltip-row", attr:data-indicator = indicator.name()) {
                    {swatch}
                    row(class = "zui-chart__tooltip-body") {
                        text(class = "zui-chart__tooltip-name") {{entry.name}}
                        text(class = "zui-chart__tooltip-value") {{tick_label(entry.value)}}
                    }
                }
            })
        })
        .collect();

    view! {
        box(class = "zui-chart__tooltip", {..attrs}, class = class) {
            {heading}
            column(class = "zui-chart__tooltip-rows") {{rows}}
        }
    }
}
