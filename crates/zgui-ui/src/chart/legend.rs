//! The key naming each series.

use zgui::prelude::*;
use zgui::view::CustomPropertyName;
use zgui::{component, view};

/// Which side of the plot a key sits on.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum LegendAlign {
    /// Under the plot, which is where a key goes unless a caller says otherwise.
    #[default]
    Bottom,
    /// Over it.
    Top,
}

impl LegendAlign {
    /// The value written to `data-align`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
        }
    }
}

/// One entry of a key: a series and the colour it is drawn in.
#[derive(Clone, PartialEq, Debug)]
pub struct LegendEntry {
    /// What the series is called.
    pub name: String,
    /// Which colour it is drawn in, as the custom property the sheet paints from.
    pub tone: String,
}

impl LegendEntry {
    /// One entry, for a series called `name` drawn in `tone`.
    #[must_use]
    pub fn new(name: impl Into<String>, tone: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tone: tone.into(),
        }
    }
}

/// The key naming a chart's series.
///
/// Written as its own component so that a caller who draws their own plot still has the key that
/// goes with it, and so that a chart with several plots has one key rather than one each.
#[component]
pub fn ChartLegendContent(
    /// The series to name, in the order they are drawn.
    entries: Vec<LegendEntry>,
    /// Which side of the plot the key is on, which decides which side its space goes.
    #[prop(default = LegendAlign::Bottom)]
    align: LegendAlign,
    /// What the key is called, for a reader.
    #[prop(into, default = String::from("Series"))]
    label: String,
    /// Classes merged after the key's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    let keys: Vec<AnyView> = entries
        .into_iter()
        .map(|entry| {
            let tone = entry.tone.clone();
            let name = entry.name.clone();
            AnyView::new(view! {
                row(class = "zui-chart__key", a11y:role = Role::ListItem, a11y:label = name.clone()) {
                    box(
                        class = "zui-chart__swatch",
                        a11y:hidden = true,
                        {..Attrs::new().custom_property(
                            CustomPropertyName::new("zui-chart-tone"),
                            move || Some(tone.clone()),
                        )}
                    )
                    text {{name}}
                }
            })
        })
        .collect();

    view! {
        row(
            class = "zui-chart__legend",
            attr:data-align = align.name(),
            a11y:role = Role::List,
            a11y:label = label,
            {..attrs},
            class = class
        ) {
            {keys}
        }
    }
}
