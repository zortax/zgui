//! One module per family of components, each producing the panels that show it off.
//!
//! Only the props types are re-exported. `view!` lowers `<Atoms/>` to `AtomsProps::builder()`, so
//! the props type is the name a call site resolves and importing the component function beside it
//! would be an unused import.

mod artwork;
mod asset;
mod atoms;
mod choices;
mod composites;
mod data;
mod disclosure;
mod feedback;
mod fields;
mod menus;
mod navigation;
mod overlays;
mod surfaces;
mod svg;
mod text;

pub(crate) use crate::section::artwork::ArtworkProps;
pub(crate) use crate::section::atoms::AtomsProps;
pub(crate) use crate::section::choices::ChoicesProps;
pub(crate) use crate::section::composites::CompositesProps;
pub(crate) use crate::section::data::DataProps;
pub(crate) use crate::section::disclosure::DisclosureProps;
pub(crate) use crate::section::feedback::FeedbackProps;
pub(crate) use crate::section::fields::FieldsProps;
pub(crate) use crate::section::menus::MenusProps;
pub(crate) use crate::section::navigation::NavigationProps;
pub(crate) use crate::section::overlays::OverlaysProps;
pub(crate) use crate::section::surfaces::SurfacesProps;
pub(crate) use crate::section::svg::SvgProps;
pub(crate) use crate::section::text::StyledTextProps;
