//! Naming a raster, rather than saying where it currently is.
//!
//! # Why a name and not a rectangle
//!
//! A sprite samples a rectangle of a texture. That rectangle is not a property of the content: it
//! is where an allocator happened to put it, it changes when the content is evicted and rasterised
//! again, and it means nothing at all once the texture it indexes has been destroyed. Building the
//! rectangle into the instance at the moment the instance is built therefore forces two things
//! together that have no reason to be together — deciding *what* a frame draws, and knowing *where*
//! everything it draws has been placed.
//!
//! A [`ResourceKey`] is the content's own name. A frame may push a sprite that carries one, and the
//! placement is filled in later from a [`ResourceRegistry`], by a pass over exactly the instances
//! that need it. That pass is per instance, at record time. The alternative — carrying the key to
//! the device and resolving it in the shader — is one more storage read per *fragment*, on the
//! commonest primitive there is, and is refused for that reason.
//!
//! # Why a generation
//!
//! An unqualified name is how a cache serves one piece of content's pixels under another's name.
//! Everything cached is discarded together — a lost device destroys every texture at once — so a
//! single counter of those moments qualifies every name at once, and a key from before one can
//! never be resolved by a registry from after it.

mod registry;

use zgui_atlas::{AtlasKey, TextureKind};

pub use crate::resource::registry::ResourceRegistry;

/// How many times everything cached has been discarded.
///
/// A key carries the generation it was made under, and a registry resolves only keys of its own,
/// which is what stops a name outliving the pixels it was given for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceGeneration(u32);

impl ResourceGeneration {
    /// The generation nothing has been discarded in.
    pub const FIRST: Self = Self(0);

    /// The generation after this one.
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The generation as a plain number, for an encoding that has to carry it as one.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The generation a plain number names.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// What a raster is made of, which decides the pool it lives in and what its texels mean.
pub type ResourceKind = TextureKind;

/// A raster named before anything says where it is.
///
/// The hash is the caller's own identity for the content — a glyph's face, size, phase and id, or a
/// decoded image's node — and is compared and never interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceKey {
    /// What the raster is made of.
    kind: ResourceKind,
    /// The content's identity.
    hash: u64,
    /// Which lifetime of the cache the name belongs to.
    generation: ResourceGeneration,
}

impl ResourceKey {
    /// The name of `hash`'s content of `kind`, made in `generation`.
    pub const fn new(kind: ResourceKind, hash: u64, generation: ResourceGeneration) -> Self {
        Self {
            kind,
            hash,
            generation,
        }
    }

    /// The name of whatever an atlas caches under `key`, in `generation`.
    pub const fn of(key: AtlasKey, generation: ResourceGeneration) -> Self {
        Self::new(key.kind(), key.handle(), generation)
    }

    /// What the raster is made of.
    pub const fn kind(self) -> ResourceKind {
        self.kind
    }

    /// The content's identity.
    pub const fn hash(self) -> u64 {
        self.hash
    }

    /// Which lifetime of the cache the name belongs to.
    pub const fn generation(self) -> ResourceGeneration {
        self.generation
    }

    /// The name a cache holding the content would file it under.
    pub const fn atlas_key(self) -> AtlasKey {
        AtlasKey::new(self.hash, self.kind)
    }
}
