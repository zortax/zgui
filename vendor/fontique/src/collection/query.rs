// Copyright 2024 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Query support.

use crate::{Charmap, CharmapIndex};

use super::super::{Collection, SourceCache};

use alloc::vec::Vec;
use parlance::Script;

use super::{
    super::{Attributes, Blob, FallbackKey, FamilyId, FamilyInfo, GenericFamily, Synthesis},
    Inner,
};

// ZGUI-PATCH: the state outlives each query. Families resolved and faces loaded for one query
// are reused by the next one that asks for the same families under the same attributes, until the
// collection changes. Upstream clears it in `Query::new` and again on drop.
#[derive(Clone, Default)]
pub(super) struct QueryState {
    families: Vec<CachedFamily>,
    /// Family lists set before the current one, most recent last, so a query that alternates
    /// between a few lists keeps what each one loaded.
    kept: Vec<Vec<CachedFamily>>,
    /// The identifiers a `set_families` call asks for, collected before they are compared.
    requested: Vec<FamilyId>,
    fallback_families: Vec<CachedFamily>,
    /// The attributes the loaded faces in both lists were matched against.
    loaded: Attributes,
    /// The key `fallback_families` was built for.
    fallbacks: Option<FallbackKey>,
    /// The collection generation the state was built against.
    generation: u64,
}

impl QueryState {
    fn clear(&mut self) {
        self.families.clear();
        self.kept.clear();
        self.fallback_families.clear();
        self.loaded = Attributes::default();
        self.fallbacks = None;
    }
}

// ZGUI-PATCH: how many family lists besides the current one keep their loaded faces.
const KEPT_LISTS: usize = 3;

/// State for font selection.
///
/// Instances of this can be obtained from [`Collection::query`].
pub struct Query<'a> {
    collection: &'a mut Inner,
    state: &'a mut QueryState,
    source_cache: &'a mut SourceCache,
    attributes: Attributes,
    fallbacks: Option<FallbackKey>,
    // ZGUI-PATCH: whether this query set its families. The kept list belongs to an earlier query
    // until it does, and a query that sets none matches none.
    families_set: bool,
}

impl<'a> Query<'a> {
    pub(super) fn new(collection: &'a mut Collection, source_cache: &'a mut SourceCache) -> Self {
        // ZGUI-PATCH: keep the state unless the collection changed since it was built.
        let generation = collection.inner.generation();
        if collection.query_state.generation != generation {
            collection.query_state.clear();
            collection.query_state.generation = generation;
        }
        Self {
            collection: &mut collection.inner,
            state: &mut collection.query_state,
            source_cache,
            attributes: Attributes::default(),
            fallbacks: None,
            families_set: false,
        }
    }

    /// Sets the ordered sequence of families to match against.
    pub fn set_families<'f, I>(&mut self, families: I)
    where
        I: IntoIterator,
        I::Item: Into<QueryFamily<'f>>,
    {
        // ZGUI-PATCH: a list asked for again — the current one or one of the last few — keeps the
        // families and faces it loaded.
        let state = &mut *self.state;
        state.requested.clear();
        for family in families {
            let family = family.into();
            match family {
                QueryFamily::Named(name) => {
                    if let Some(id) = self.collection.family_id(name) {
                        state.requested.push(id);
                    }
                }
                QueryFamily::Id(id) => {
                    state.requested.push(id);
                }
                QueryFamily::Generic(generic) => {
                    state
                        .requested
                        .extend(self.collection.generic_families(generic));
                }
            }
        }
        let same = |list: &[CachedFamily], ids: &[FamilyId]| {
            list.len() == ids.len() && list.iter().zip(ids).all(|(family, id)| family.id == *id)
        };
        if !same(&state.families, &state.requested) {
            let previous = core::mem::take(&mut state.families);
            if let Some(index) = state
                .kept
                .iter()
                .position(|list| same(list, &state.requested))
            {
                state.families = state.kept.remove(index);
            } else {
                if state.kept.len() >= KEPT_LISTS {
                    state.families = state.kept.remove(0);
                    state.families.clear();
                }
                state
                    .families
                    .extend(state.requested.iter().copied().map(CachedFamily::new));
            }
            state.kept.push(previous);
        }
        self.families_set = true;
    }

    /// Sets the primary attributes to match against.
    pub fn set_attributes(&mut self, attributes: Attributes) {
        // ZGUI-PATCH: loaded faces are dropped lazily, in `matches_with`, when they were matched
        // against other attributes.
        self.attributes = attributes;
    }

    /// Sets the script and locale for fallback fonts.
    pub fn set_fallbacks(&mut self, key: impl Into<FallbackKey>) {
        let key = key.into();
        // ZGUI-PATCH: compared against the kept list's key, so a repeated key keeps the list.
        if self.state.fallbacks != Some(key) {
            self.state.fallback_families.clear();
            self.state.fallback_families.extend(
                self.collection
                    .fallback_families(key)
                    .map(CachedFamily::new),
            );
            // HACK: always add a Han font to the fallback list to capture
            // punctuation in the common script
            // See <https://github.com/linebender/parley/issues/597>
            self.state.fallback_families.extend(
                self.collection
                    .fallback_families(FallbackKey::new(Script::from_bytes(*b"Hani"), None))
                    .map(CachedFamily::new),
            );
            self.state.fallbacks = Some(key);
        }
        self.fallbacks = Some(key);
    }

    /// Invokes the given callback with all fonts that match the current
    /// settings.
    ///
    /// Return [`QueryStatus::Stop`] to end iterating over the matching
    /// fonts or [`QueryStatus::Continue`] to continue iterating.
    pub fn matches_with(&mut self, mut f: impl FnMut(&QueryFont) -> QueryStatus) {
        // ZGUI-PATCH: faces matched against other attributes are dropped here.
        if self.state.loaded != self.attributes {
            for family in &mut self.state.families {
                family.clear_fonts();
            }
            for family in self.state.kept.iter_mut().flatten() {
                family.clear_fonts();
            }
            for family in &mut self.state.fallback_families {
                family.clear_fonts();
            }
            self.state.loaded = self.attributes;
        }
        // ZGUI-PATCH: only the lists this query set take part.
        let families = if self.families_set {
            &mut self.state.families[..]
        } else {
            &mut []
        };
        let fallback_families = if self.fallbacks.is_some() {
            &mut self.state.fallback_families[..]
        } else {
            &mut []
        };
        for family in families.iter_mut().chain(fallback_families.iter_mut()) {
            match &mut family.family {
                Entry::Error => continue,
                Entry::Ok(..) => {}
                status @ Entry::Vacant => {
                    if let Some(info) = self.collection.family(family.id) {
                        *status = Entry::Ok(info);
                    } else {
                        *status = Entry::Error;
                        continue;
                    }
                }
            }
            let Entry::Ok(family_info) = &family.family else {
                continue;
            };
            let mut best_index = None;
            if let Some(font) = load_font(
                family_info,
                self.attributes,
                &mut family.best,
                false,
                self.source_cache,
            ) {
                best_index = Some(font.family.1);
                if f(font) == QueryStatus::Stop {
                    return;
                }
            }
            // Don't invoke for the default font if it's the same as the
            // best match.
            if best_index == Some(family_info.default_font_index()) {
                continue;
            }
            if let Some(font) = load_font(
                family_info,
                self.attributes,
                &mut family.default,
                true,
                self.source_cache,
            ) {
                if f(font) == QueryStatus::Stop {
                    return;
                }
            }
        }
    }
}

/// Determines whether a font query operation will continue.
///
/// See [`Query::matches_with`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum QueryStatus {
    /// Query should continue with the next font.
    Continue,
    /// Query should stop.
    Stop,
}

/// Family descriptor for a font query.
///
/// This allows [`Query::set_families`] to
/// take a variety of family types.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum QueryFamily<'a> {
    /// A named font family.
    Named(&'a str),
    /// An identifier for a font family.
    Id(FamilyId),
    /// A generic font family.
    Generic(GenericFamily),
}

impl<'a> From<&'a str> for QueryFamily<'a> {
    fn from(value: &'a str) -> Self {
        Self::Named(value)
    }
}

impl From<FamilyId> for QueryFamily<'static> {
    fn from(value: FamilyId) -> Self {
        Self::Id(value)
    }
}

impl From<GenericFamily> for QueryFamily<'static> {
    fn from(value: GenericFamily) -> Self {
        Self::Generic(value)
    }
}

/// Candidate font generated by a [`Query`].
#[derive(Clone, Debug)]
pub struct QueryFont {
    /// Family identifier and index of the font in the family font list.
    pub family: (FamilyId, usize),
    /// Blob containing the font data.
    pub blob: Blob<u8>,
    /// Index of a font in a font collection (`ttc`) file.
    pub index: u32,
    /// Synthesis suggestions for this font based on the requested attributes.
    pub synthesis: Synthesis,
    /// Data used for constructing a character map for this font.
    pub charmap_index: CharmapIndex,
}

impl QueryFont {
    /// Attempts to construct a [Charmap] for this font.
    pub fn charmap(&self) -> Option<Charmap<'_>> {
        self.charmap_index.charmap(self.blob.as_ref())
    }
}

fn load_font<'a>(
    family: &FamilyInfo,
    attributes: Attributes,
    font: &'a mut Entry<QueryFont>,
    is_default: bool,
    source_cache: &mut SourceCache,
) -> Option<&'a QueryFont> {
    match font {
        Entry::Error => None,
        Entry::Ok(font) => Some(font),
        status @ Entry::Vacant => {
            // Set to error in case we fail. This simplifies
            // the following code.
            *status = Entry::Error;
            let family_index = if is_default {
                family.default_font_index()
            } else {
                family.match_index(attributes.width, attributes.style, attributes.weight, true)?
            };
            let font_info = family.fonts().get(family_index)?;
            let blob = font_info.load(Some(source_cache))?;
            let blob_index = font_info.index();
            let synthesis =
                font_info.synthesis(attributes.width, attributes.style, attributes.weight);
            *status = Entry::Ok(QueryFont {
                family: (family.id(), family_index),
                blob: blob.clone(),
                index: blob_index,
                synthesis,
                charmap_index: font_info.charmap_index(),
            });
            if let Entry::Ok(font) = status {
                Some(font)
            } else {
                None
            }
        }
    }
}

#[derive(Clone)]
struct CachedFamily {
    id: FamilyId,
    family: Entry<FamilyInfo>,
    best: Entry<QueryFont>,
    default: Entry<QueryFont>,
}

impl CachedFamily {
    fn new(id: FamilyId) -> Self {
        Self {
            id,
            family: Entry::Vacant,
            best: Entry::Vacant,
            default: Entry::Vacant,
        }
    }

    fn clear_fonts(&mut self) {
        self.best = Entry::Vacant;
        self.default = Entry::Vacant;
    }
}

#[derive(Clone)]
enum Entry<T> {
    Ok(T),
    Vacant,
    Error,
}
