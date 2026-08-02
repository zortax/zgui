//! The track, the slides and the two arrows.

mod arrow;
mod content;
mod item;

pub use crate::carousel::parts::arrow::{
    CarouselNext, CarouselNextProps, CarouselPrevious, CarouselPreviousProps,
};
pub use crate::carousel::parts::content::{CarouselContent, CarouselContentProps};
pub use crate::carousel::parts::item::{CarouselItem, CarouselItemProps};
