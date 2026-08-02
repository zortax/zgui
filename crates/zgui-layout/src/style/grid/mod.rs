//! The properties grid layout reads beyond the core set.

pub mod names;
pub mod placement;
pub mod repetition;
pub mod tracks;

use taffy::{
    AlignContent, AlignItems, AlignSelf, GenericGridTemplateComponent, GridAutoFlow,
    GridContainerStyle, GridItemStyle, GridTemplateArea, JustifyContent, LengthPercentage, Line,
    Size, TrackSizingFunction,
};
use zgui_css::values::grid::{
    GridAutoFlowValue, GridTemplateAreasValue, GridTemplateComponentValue, TrackListEntry,
    TrackListValue, TrackSizeValue,
};
use zgui_css::values::length::LengthPercentage as CssLengthPercentage;
use zgui_interned::Ident;

use crate::style::StyleRef;
use crate::style::convert::align;
use crate::style::gap::gap_of;
use crate::style::grid::names::{LineNamesIter, line_names};
use crate::style::grid::repetition::Repetition;

/// One track list, walked lazily.
#[derive(Clone, Debug)]
pub struct TemplateTracks<'a> {
    /// What is left to yield.
    entries: core::slice::Iter<'a, TrackListEntry<CssLengthPercentage, i32>>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl<'a> Iterator for TemplateTracks<'a> {
    type Item = GenericGridTemplateComponent<Ident, Repetition<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(match self.entries.next()? {
            TrackListEntry::TrackSize(size) => {
                GenericGridTemplateComponent::Single(tracks::track(size, self.scale))
            }
            TrackListEntry::TrackRepeat(repeat) => {
                GenericGridTemplateComponent::Repeat(Repetition::new(repeat, self.scale))
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

impl ExactSizeIterator for TemplateTracks<'_> {}

/// One implicit-track list, walked lazily.
#[derive(Clone, Debug)]
pub struct AutoTracks<'a> {
    /// What is left to yield.
    sizes: core::slice::Iter<'a, TrackSizeValue>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl Iterator for AutoTracks<'_> {
    type Item = TrackSizingFunction;

    fn next(&mut self) -> Option<TrackSizingFunction> {
        self.sizes
            .next()
            .map(|size| tracks::track(size, self.scale))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sizes.size_hint()
    }
}

impl ExactSizeIterator for AutoTracks<'_> {}

impl<'a> StyleRef<'a> {
    /// The explicit tracks of one axis, if any were written.
    ///
    /// `subgrid` and `masonry` have no representation and read as though no tracks were written,
    /// which is the value they would have had before either was added to CSS.
    fn template(self, component: &'a GridTemplateComponentValue) -> Option<TemplateTracks<'a>> {
        match component {
            GridTemplateComponentValue::TrackList(list) => Some(TemplateTracks {
                entries: list.values.iter(),
                scale: self.scale(),
            }),
            GridTemplateComponentValue::None
            | GridTemplateComponentValue::Subgrid(_)
            | GridTemplateComponentValue::Masonry => None,
        }
    }

    /// The line names of one axis, or none if no line in it is named.
    fn names(
        self,
        pick: impl FnOnce(&crate::node::grid_names::GridNames) -> Option<&[Vec<Ident>]>,
    ) -> Option<LineNamesIter<'a>> {
        let names = self.node().grid_names().and_then(pick)?;
        Some(line_names(names))
    }
}

impl GridContainerStyle for StyleRef<'_> {
    type Repetition<'a>
        = Repetition<'a>
    where
        Self: 'a;
    type TemplateTrackList<'a>
        = TemplateTracks<'a>
    where
        Self: 'a;
    type AutoTrackList<'a>
        = AutoTracks<'a>
    where
        Self: 'a;
    type TemplateLineNames<'a>
        = LineNamesIter<'a>
    where
        Self: 'a;
    type GridTemplateAreas<'a>
        = core::iter::Cloned<core::slice::Iter<'a, GridTemplateArea<Ident>>>
    where
        Self: 'a;

    fn grid_template_rows(&self) -> Option<Self::TemplateTrackList<'_>> {
        (*self).template(&(*self).position_group().grid_template_rows)
    }

    fn grid_template_columns(&self) -> Option<Self::TemplateTrackList<'_>> {
        (*self).template(&(*self).position_group().grid_template_columns)
    }

    fn grid_auto_rows(&self) -> Self::AutoTrackList<'_> {
        AutoTracks {
            sizes: (*self).position_group().grid_auto_rows.0.iter(),
            scale: self.scale(),
        }
    }

    fn grid_auto_columns(&self) -> Self::AutoTrackList<'_> {
        AutoTracks {
            sizes: (*self).position_group().grid_auto_columns.0.iter(),
            scale: self.scale(),
        }
    }

    fn grid_template_areas(&self) -> Option<Self::GridTemplateAreas<'_>> {
        let areas = &(*self).node().grid_names()?.areas;
        (!areas.is_empty()).then(|| areas.iter().cloned())
    }

    fn grid_template_column_names(&self) -> Option<Self::TemplateLineNames<'_>> {
        (*self).names(|names| names.column_lines())
    }

    fn grid_template_row_names(&self) -> Option<Self::TemplateLineNames<'_>> {
        (*self).names(|names| names.row_lines())
    }

    fn grid_auto_flow(&self) -> GridAutoFlow {
        let flow = self.position_group().grid_auto_flow;
        let dense = flow.contains(GridAutoFlowValue::DENSE);
        match (flow.contains(GridAutoFlowValue::COLUMN), dense) {
            (false, false) => GridAutoFlow::Row,
            (false, true) => GridAutoFlow::RowDense,
            (true, false) => GridAutoFlow::Column,
            (true, true) => GridAutoFlow::ColumnDense,
        }
    }

    fn gap(&self) -> Size<LengthPercentage> {
        gap_of(*self)
    }

    fn align_content(&self) -> Option<AlignContent> {
        align::align_content(self.position_group().align_content.primary(), self.is_rtl())
    }

    fn justify_content(&self) -> Option<JustifyContent> {
        align::align_content(
            self.position_group().justify_content.primary(),
            self.is_rtl(),
        )
    }

    fn align_items(&self) -> Option<AlignItems> {
        align::align_items(self.position_group().align_items.0, self.is_rtl())
    }

    fn justify_items(&self) -> Option<AlignItems> {
        align::justify_items(
            (self.position_group().justify_items.computed.0).0,
            self.is_rtl(),
        )
    }
}

impl GridItemStyle for StyleRef<'_> {
    fn grid_row(&self) -> Line<taffy::GridPlacement<Ident>> {
        let position = self.position_group();
        placement::line(&position.grid_row_start, &position.grid_row_end)
    }

    fn grid_column(&self) -> Line<taffy::GridPlacement<Ident>> {
        let position = self.position_group();
        placement::line(&position.grid_column_start, &position.grid_column_end)
    }

    fn align_self(&self) -> Option<AlignSelf> {
        align::align_items(self.position_group().align_self.0, self.is_rtl())
    }

    fn justify_self(&self) -> Option<AlignSelf> {
        align::align_items(self.position_group().justify_self.0, self.is_rtl())
    }
}

/// Translates one grid's line names and named areas into this framework's own identifiers.
///
/// Done once when the box is built, because the layout algorithms want references to names and a
/// name produced on the fly would have nothing to be a reference to.
pub fn resolve_names(style: &zgui_css::ComputedStyle) -> crate::node::grid_names::GridNames {
    let position = style.get_position();
    crate::node::grid_names::GridNames {
        rows: template_names(&position.grid_template_rows),
        columns: template_names(&position.grid_template_columns),
        areas: template_areas(&position.grid_template_areas),
    }
}

/// The line names written against one axis's explicit tracks.
fn template_names(component: &GridTemplateComponentValue) -> Vec<Vec<Ident>> {
    match component {
        GridTemplateComponentValue::TrackList(list) => names_of(list),
        GridTemplateComponentValue::None
        | GridTemplateComponentValue::Subgrid(_)
        | GridTemplateComponentValue::Masonry => Vec::new(),
    }
}

/// The line names of one track list, one entry per line.
fn names_of(list: &TrackListValue) -> Vec<Vec<Ident>> {
    list.line_names
        .iter()
        .map(|line| {
            line.iter()
                .map(|name| Ident::new(name.0.as_ref()))
                .collect()
        })
        .collect()
}

/// The rectangles `grid-template-areas` names.
fn template_areas(value: &GridTemplateAreasValue) -> Vec<GridTemplateArea<Ident>> {
    let GridTemplateAreasValue::Areas(areas) = value else {
        return Vec::new();
    };
    areas
        .0
        .areas
        .iter()
        .map(|area| GridTemplateArea {
            name: Ident::new(area.name.as_ref()),
            row_start: area.rows.start as u16,
            row_end: area.rows.end as u16,
            column_start: area.columns.start as u16,
            column_end: area.columns.end as u16,
        })
        .collect()
}
