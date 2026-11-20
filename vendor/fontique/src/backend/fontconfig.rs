// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use core::{
    ffi::{CStr, c_char},
    marker::PhantomData,
    ptr::NonNull,
};
use std::{
    borrow::Cow,
    ffi::{CString, OsStr},
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::Arc,
};

#[cfg(feature = "fontconfig-dlopen")]
use fontconfig_sys::statics::{LIB, LIB_RESULT};
use fontconfig_sys::{
    FcChar8, FcCharSet, FcConfig, FcFontSet, FcLangSet, FcMatchKind, FcMatchPattern, FcPattern,
    FcResult, FcResultMatch, FcResultNoId, FcResultNoMatch, FcResultOutOfMemory,
    FcResultTypeMismatch, FcSetSystem, FcStrList,
    constants::{FC_CHARSET, FC_FAMILY, FC_FILE, FC_INDEX, FC_LANG, FC_SLANT, FC_WEIGHT, FC_WIDTH},
    ffi_dispatch,
};
#[cfg(not(feature = "fontconfig-dlopen"))]
use fontconfig_sys::{
    FcCharSetAddChar, FcCharSetCopy, FcCharSetCreate, FcCharSetDestroy, FcConfigBuildFonts,
    FcConfigDestroy, FcConfigGetFonts, FcConfigReference, FcConfigSubstitute, FcFontMatch,
    FcFontRenderPrepare, FcFontSetDestroy, FcFontSort, FcInitLoadConfig, FcLangSetAdd,
    FcLangSetCopy, FcLangSetCreate, FcLangSetDestroy, FcNameUnparse, FcPatternAddCharSet,
    FcPatternAddLangSet, FcPatternAddString, FcPatternCreate, FcPatternDestroy,
    FcPatternGetInteger, FcPatternGetString, FcPatternReference, FcStrFree,
};
// ZGUI-PATCH: what the on-disk cache stamps.
#[cfg(not(feature = "fontconfig-dlopen"))]
use fontconfig_sys::{
    FcConfigGetConfigFiles, FcConfigGetFontDirs, FcGetVersion, FcStrListDone, FcStrListNext,
};

use hashbrown::{HashMap, HashSet, hash_map::Entry};
use smallvec::SmallVec;

// ZGUI-PATCH: an on-disk copy of what this backend finds.
use std::{borrow::ToOwned, string::String, vec::Vec};
#[path = "fontconfig_cache.rs"]
mod cache;

use crate::{
    FallbackKey, FamilyId, FamilyInfo, FontInfo, FontStyle, FontWeight, FontWidth,
    FromFontconfig as _, GenericFamily, Script, ScriptExt,
    family_name::{FamilyName, FamilyNameMap},
    generic::GenericFamilyMap,
    source::SourcePathMap,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MatchErr {
    NoMatch,
    TypeMismatch,
    NoId,
    OutOfMemory,
    Other,
}

impl MatchErr {
    fn from_raw(raw: FcResult) -> Self {
        #[allow(non_upper_case_globals)]
        match raw {
            FcResultNoMatch => Self::NoMatch,
            FcResultTypeMismatch => Self::TypeMismatch,
            FcResultNoId => Self::NoId,
            FcResultOutOfMemory => Self::OutOfMemory,
            _ => Self::Other,
        }
    }
}

type MatchResult<T> = Result<T, MatchErr>;

/// Ownership for refcounted Fontconfig objects. Used to track if a given
/// fontconfig function returns an object that it owns or is passing its
/// ownership to us.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ownership {
    /// This object is owned by Fontconfig and needs to be freed by it.
    Fontconfig,
    /// This object is owned by the application.
    Application,
}

/// Wrapper for an `FcPattern`.
struct Pattern {
    inner: NonNull<FcPattern>,
}

impl Pattern {
    fn new() -> Option<Self> {
        Some(unsafe {
            Self::from_raw(
                ffi_dispatch!(feature = "fontconfig-dlopen", LIB, FcPatternCreate,),
                Ownership::Application,
            )?
        })
    }

    unsafe fn from_raw(raw: *mut FcPattern, ownership: Ownership) -> Option<Self> {
        let inner = NonNull::new(raw)?;
        // Don't free this object when we are dropped.
        if ownership == Ownership::Fontconfig {
            unsafe {
                ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcPatternReference,
                    inner.as_ptr()
                );
            }
        }
        Some(Self { inner })
    }

    fn add_string(&mut self, object: &CStr, s: &CStr) -> bool {
        // All objects passed to FcPatternAddWhatever are cloned.
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcPatternAddString,
                self.inner.as_ptr(),
                object.as_ptr(),
                s.as_ptr() as *const _
            ) != 0
        }
    }

    fn add_charset(&mut self, object: &CStr, s: &CharSet) -> bool {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcPatternAddCharSet,
                self.inner.as_ptr(),
                object.as_ptr(),
                s.inner.as_ptr()
            ) != 0
        }
    }

    fn add_langset(&mut self, object: &CStr, s: &LangSet) -> bool {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcPatternAddLangSet,
                self.inner.as_ptr(),
                object.as_ptr(),
                s.inner.as_ptr()
            ) != 0
        }
    }

    fn get_string<'a>(&'a self, object: &CStr, n: u32) -> MatchResult<Cow<'a, str>> {
        Ok(self.get_c_string(object, n)?.to_string_lossy())
    }

    fn get_c_string<'a>(&'a self, object: &CStr, n: u32) -> MatchResult<&'a CStr> {
        let mut dest: *mut FcChar8 = std::ptr::null_mut();
        let result = unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcPatternGetString,
                self.inner.as_ptr(),
                object.as_ptr(),
                n.try_into().map_err(|_| MatchErr::Other)?,
                &raw mut dest
            )
        };
        if result != FcResultMatch {
            return Err(MatchErr::from_raw(result));
        }
        let dest = NonNull::new(dest).ok_or(MatchErr::Other)?;
        Ok(unsafe { CStr::from_ptr(dest.as_ptr() as *const _) })
    }

    fn get_int(&self, object: &CStr, n: u32) -> MatchResult<i32> {
        let mut dest = 0;
        let result = unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcPatternGetInteger,
                self.inner.as_ptr(),
                object.as_ptr(),
                n.try_into().map_err(|_| MatchErr::Other)?,
                &raw mut dest
            )
        };
        if result != FcResultMatch {
            return Err(MatchErr::from_raw(result));
        }
        Ok(dest)
    }
}

impl Clone for Pattern {
    fn clone(&self) -> Self {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcPatternReference,
                self.inner.as_ptr()
            );
        };
        Self { inner: self.inner }
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcPatternDestroy,
                self.inner.as_ptr()
            );
        };
    }
}

impl std::fmt::Debug for Pattern {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match NonNull::new(unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcNameUnparse,
                self.inner.as_ptr()
            )
        }) {
            Some(unparsed) => {
                let res = f.write_str(unsafe {
                    &CStr::from_ptr(unparsed.as_ptr() as *const c_char).to_string_lossy()
                });
                unsafe {
                    ffi_dispatch!(
                        feature = "fontconfig-dlopen",
                        LIB,
                        FcStrFree,
                        unparsed.as_ptr()
                    );
                };
                res
            }
            None => f.debug_struct("Pattern").finish(),
        }
    }
}

struct FontSet<'a> {
    inner: NonNull<FcFontSet>,
    ownership: Ownership,
    // If an `FcFontSet` is created from an `FcPattern`, it will reference that
    // pattern's data. Well, maybe. The docs say "The returned FcFontSet
    // references FcPattern structures which may be shared by the return value
    // from multiple FcFontSort calls, applications cannot modify these
    // patterns." It's unclear whether this refers to actual lifetime/ownership
    // semantics or if everything's properly refcounted and you're just not
    // allowed to mutate them.
    _parent: PhantomData<&'a ()>,
}

impl FontSet<'_> {
    unsafe fn from_raw(raw: *mut FcFontSet, ownership: Ownership) -> Option<Self> {
        let inner = NonNull::new(raw)?;
        Some(Self {
            inner,
            ownership,
            _parent: PhantomData,
        })
    }

    fn iter(&self) -> FontSetIter<'_> {
        FontSetIter {
            i: 0,
            font_set: self,
        }
    }
}

impl Drop for FontSet<'_> {
    fn drop(&mut self) {
        if self.ownership == Ownership::Application {
            unsafe {
                ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcFontSetDestroy,
                    self.inner.as_ptr()
                );
            };
        }
    }
}

struct FontSetIter<'a> {
    i: usize,
    font_set: &'a FontSet<'a>,
}

impl Iterator for FontSetIter<'_> {
    type Item = Pattern;

    fn next(&mut self) -> Option<Self::Item> {
        let font_set = self.font_set.inner.as_ptr();
        if self.i >= unsafe { (*font_set).nfont }.try_into().ok()? {
            None
        } else {
            let pattern: *mut FcPattern = unsafe { *(*font_set).fonts.add(self.i) };
            self.i += 1;
            Some(unsafe { Pattern::from_raw(pattern, Ownership::Fontconfig) }?)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let nfont: Result<usize, _> = unsafe { (*self.font_set.inner.as_ptr()).nfont }.try_into();
        let Ok(nfont) = nfont else {
            return (0, None);
        };
        (nfont, Some(nfont))
    }
}

struct LangSet {
    inner: NonNull<FcLangSet>,
}

impl LangSet {
    fn new() -> Option<Self> {
        let inner = NonNull::new(unsafe {
            ffi_dispatch!(feature = "fontconfig-dlopen", LIB, FcLangSetCreate,)
        })?;
        Some(Self { inner })
    }

    fn add(&mut self, lang: &CStr) -> bool {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcLangSetAdd,
                self.inner.as_ptr(),
                lang.as_ptr() as *const _
            ) != 0
        }
    }
}

impl Drop for LangSet {
    fn drop(&mut self) {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcLangSetDestroy,
                self.inner.as_ptr()
            );
        };
    }
}

impl Clone for LangSet {
    fn clone(&self) -> Self {
        Self {
            inner: unsafe {
                NonNull::new(ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcLangSetCopy,
                    self.inner.as_ptr()
                ))
                .unwrap()
            },
        }
    }
}

struct CharSet {
    inner: NonNull<FcCharSet>,
}

impl CharSet {
    fn new() -> Option<Self> {
        let inner = NonNull::new(unsafe {
            ffi_dispatch!(feature = "fontconfig-dlopen", LIB, FcCharSetCreate,)
        })?;
        Some(Self { inner })
    }

    fn add(&mut self, c: char) -> bool {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcCharSetAddChar,
                self.inner.as_ptr(),
                c as u32
            ) != 0
        }
    }
}

impl Drop for CharSet {
    fn drop(&mut self) {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcCharSetDestroy,
                self.inner.as_ptr()
            );
        };
    }
}

impl Clone for CharSet {
    fn clone(&self) -> Self {
        Self {
            inner: unsafe {
                NonNull::new(ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcCharSetCopy,
                    self.inner.as_ptr()
                ))
                .unwrap()
            },
        }
    }
}

struct Config {
    inner: NonNull<FcConfig>,
}

impl Config {
    // ZGUI-PATCH: one place that loads a configuration, for the eager and the lazy path.
    fn load() -> Option<Self> {
        crate::system_cache::CONFIG_LOADS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let config =
            unsafe { ffi_dispatch!(feature = "fontconfig-dlopen", LIB, FcInitLoadConfig,) };
        // fontconfig returns a new config object each time we call FcInitLoadConfig
        let config = unsafe { Self::from_raw(config, Ownership::Application) }?;
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcConfigBuildFonts,
                config.inner.as_ptr()
            );
        }
        Some(config)
    }

    // ZGUI-PATCH: the font directories and configuration files, for the cache stamps.
    fn reported(&self) -> cache::Reported {
        let list = |raw: *mut FcStrList| {
            let mut out = Vec::new();
            if raw.is_null() {
                return out;
            }
            loop {
                let next = unsafe {
                    ffi_dispatch!(feature = "fontconfig-dlopen", LIB, FcStrListNext, raw)
                };
                if next.is_null() {
                    break;
                }
                out.push(
                    unsafe { CStr::from_ptr(next as *const c_char) }
                        .to_bytes()
                        .to_vec(),
                );
            }
            unsafe { ffi_dispatch!(feature = "fontconfig-dlopen", LIB, FcStrListDone, raw) };
            out
        };
        let font_dirs = list(unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcConfigGetFontDirs,
                self.inner.as_ptr()
            )
        });
        let config_files = list(unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcConfigGetConfigFiles,
                self.inner.as_ptr()
            )
        });
        cache::Reported {
            font_dirs,
            config_files,
        }
    }

    unsafe fn from_raw(raw: *mut FcConfig, ownership: Ownership) -> Option<Self> {
        let inner = NonNull::new(raw)?;
        // Don't free this object when we are dropped.
        if ownership == Ownership::Fontconfig {
            unsafe {
                ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcConfigReference,
                    inner.as_ptr()
                );
            }
        }
        Some(Self { inner })
    }

    fn substitute(&self, pattern: &mut Pattern, kind: FcMatchKind) {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcConfigSubstitute,
                self.inner.as_ptr(),
                pattern.inner.as_ptr(),
                kind
            )
        };
    }

    fn font_sort<'me, 'ret, 'pat: 'ret>(
        &'me self,
        pattern: &'pat Pattern,
        trim: bool,
    ) -> MatchResult<FontSet<'ret>> {
        let mut result = 0;
        // The returned FcFontSet is for us to free.
        let font_set = unsafe {
            FontSet::from_raw(
                ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcFontSort,
                    self.inner.as_ptr(),
                    pattern.inner.as_ptr(),
                    trim as i32,
                    std::ptr::null_mut(),
                    &raw mut result
                ),
                Ownership::Application,
            )
        }
        .ok_or(MatchErr::Other)?;
        if result != FcResultMatch {
            return Err(MatchErr::from_raw(result));
        }

        Ok(font_set)
    }

    fn font_match(&self, pattern: &Pattern) -> MatchResult<Pattern> {
        let mut result = 0;
        let pattern = unsafe {
            Pattern::from_raw(
                ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcFontMatch,
                    self.inner.as_ptr(),
                    pattern.inner.as_ptr(),
                    &raw mut result
                ),
                Ownership::Application,
            )
        }
        .ok_or(MatchErr::Other)?;
        if result != FcResultMatch {
            return Err(MatchErr::from_raw(result));
        }

        Ok(pattern)
    }

    fn font_render_prepare(&self, pat: &Pattern, font: &Pattern) -> Option<Pattern> {
        unsafe {
            Pattern::from_raw(
                ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcFontRenderPrepare,
                    self.inner.as_ptr(),
                    pat.inner.as_ptr(),
                    font.inner.as_ptr()
                ),
                Ownership::Application,
            )
        }
    }
}

impl Clone for Config {
    fn clone(&self) -> Self {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcConfigReference,
                self.inner.as_ptr()
            )
        };
        Self { inner: self.inner }
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        unsafe {
            ffi_dispatch!(
                feature = "fontconfig-dlopen",
                LIB,
                FcConfigDestroy,
                self.inner.as_ptr()
            );
        };
    }
}

/// Cache wrapper that maps Unicode scripts to fontconfig [`CharSet`]s.
#[derive(Default)]
struct ScriptCharSetMap(HashMap<Script, Option<CharSet>>);

impl ScriptCharSetMap {
    fn charset_for_script(&mut self, script: Script) -> Option<&CharSet> {
        match self.0.entry(script) {
            Entry::Occupied(e) => e.into_mut().as_ref(),
            Entry::Vacant(e) => {
                let Some(sample) = script.sample() else {
                    return e.insert(None).as_ref();
                };

                let mut charset = CharSet::new()?;
                for c in sample.chars() {
                    charset.add(c);
                }
                e.insert(Some(charset)).as_ref()
            }
        }
    }
}

/// Raw access to the collection of local system fonts.
#[derive(Default)]
pub(crate) struct SystemFonts {
    pub(crate) name_map: Arc<FamilyNameMap>,
    pub(crate) generic_families: Arc<GenericFamilyMap>,
    source_cache: SourcePathMap,
    family_map: HashMap<FamilyId, Option<FamilyInfo>>,
    config: Option<Config>,
    script_charsets: ScriptCharSetMap,
    // ZGUI-PATCH: the on-disk cache, when it is on and could be read or written.
    cache: Option<cache::Live>,
    // ZGUI-PATCH: whether `config` was loaded or tried. A collection built from the cache loads
    // it at the first question the cache cannot answer.
    config_tried: bool,
}

unsafe impl Send for SystemFonts {}

impl SystemFonts {
    pub(crate) fn new() -> Self {
        // We couldn't find the fontconfig library; maybe it doesn't exist. Just
        // return a `SystemFonts` with no `config`. All our methods will return
        // `None` and shouldn't attempt any FFI calls because the first thing we
        // do is check for `config`.
        #[cfg(feature = "fontconfig-dlopen")]
        if LIB_RESULT.is_err() {
            return Self::default();
        }

        // ZGUI-PATCH: a valid cache file takes the place of the work below.
        let place = cache::Place::from_env(fontconfig_version());
        if let Some((base, live)) = place.as_ref().and_then(cache::Place::load) {
            return Self::from_cache(&base, live);
        }

        // ZGUI-PATCH: the cache trusts only stamps older than the start of the scan.
        let started = std::time::SystemTime::now();

        // Initialize the config
        let Some(config) = Config::load() else {
            return Self::default();
        };

        // Get all the fonts.

        // The fontconfig docs say this "isn't threadsafe", but this seems to be
        // related to refcounting:
        // https://gitlab.freedesktop.org/fontconfig/fontconfig/-/commit/b5bcf61fe789e66df2de609ec246cb7e4d326180
        // The source code for this function just calls `FcConfigGetCurrent` if
        // none is provided (which we do, and it's atomic anyway) and then
        // dereferences a pointer. I *think* the safety issue they're referring
        // to is that if we destroyed this config on another thread and then
        // tried to access what it returns, it would dereference a null pointer.
        // But we're not doing that.
        let Some(font_set) = (unsafe {
            FontSet::from_raw(
                ffi_dispatch!(
                    feature = "fontconfig-dlopen",
                    LIB,
                    FcConfigGetFonts,
                    config.inner.as_ptr(),
                    FcSetSystem
                ),
                Ownership::Fontconfig,
            )
        }) else {
            return Self::default();
        };

        // ZGUI-PATCH: what the cache file keeps, collected only when the cache is on.
        let mut base = place.as_ref().map(|_| cache::Base {
            names: Vec::new(),
            generics: Vec::new(),
        });
        let mut seen_names = HashSet::<Vec<String>>::new();

        // Populate the family name map.
        let mut name_map = FamilyNameMap::default();
        for pattern in font_set.iter() {
            let mut i = 0;

            let mut first_name_id = None;
            // ZGUI-PATCH: this face's names, for the cache.
            let mut names = Vec::new();
            // For fonts with more than one family name, the second one is
            // *often* (but not always) an RBIZ name.
            while let Ok(name) = pattern.get_string(FC_FAMILY, i) {
                if base.is_some() {
                    names.push(String::from(name.as_ref()));
                }
                if i == 0 {
                    // First name
                    first_name_id = Some(name_map.get_or_insert(&name).id());
                } else if let Some(first_name_id) = first_name_id {
                    name_map.add_alias(first_name_id, &name);
                }
                i += 1;
            }
            // ZGUI-PATCH: a list seen before adds nothing to the map, so the cache keeps it once.
            if let Some(base) = &mut base {
                if !names.is_empty() && seen_names.insert(names.clone()) {
                    base.names.push(names);
                }
            }
        }

        // Populate the generic family map.
        let mut generic_families = GenericFamilyMap::default();
        for (generic_family, name) in GENERIC_FAMILY_NAMES.iter().copied() {
            let mut pattern = Pattern::new().unwrap();
            pattern.add_string(FC_FAMILY, name);
            // TODO: do we need FcConfigSetDefaultSubstitute?

            config.substitute(&mut pattern, FcMatchPattern);

            // We enable the "trim" option here which ignores later fonts if
            // they provide no new Unicode coverage.
            let font_set = config.font_sort(&pattern, true).unwrap();

            // There are a lot of duplicate font families in the substituted
            // pattern. Keep track of which ones have already been added to the
            // list.
            let mut added_families = HashSet::new();
            // ZGUI-PATCH: the names that resolved, for the cache.
            let mut added_names = Vec::new();

            generic_families.append(
                generic_family,
                font_set.iter().filter_map(|font| {
                    // Not sure if FcFontRenderPrepare performs any substitutions
                    // relevant to fallback family name matching, but it's a good
                    // idea to call it just in case.
                    let font = config.font_render_prepare(&pattern, &font)?;
                    // Generic families can have more than one name, but the only
                    // one we care about is the first one.
                    let name = font.get_string(FC_FAMILY, 0).ok()?;
                    let family_name = name_map.get(name.as_ref())?;

                    if !added_families.insert(family_name.id()) {
                        return None;
                    }

                    if base.is_some() {
                        added_names.push(String::from(name.as_ref()));
                    }
                    Some(family_name.id())
                }),
            );
            if let Some(base) = &mut base {
                base.generics.push(added_names);
            }
        }

        // ZGUI-PATCH: write the cache file for the next process.
        let cache = place
            .zip(base)
            .and_then(|(place, base)| place.store(&base, &config.reported(), started));

        Self {
            name_map: Arc::new(name_map),
            generic_families: Arc::new(generic_families),
            source_cache: SourcePathMap::default(),
            family_map: HashMap::default(),
            config: Some(config),
            script_charsets: ScriptCharSetMap::default(),
            cache,
            config_tried: true,
        }
    }

    // ZGUI-PATCH: the collection from a cache file, with no fontconfig work.
    fn from_cache(base: &cache::Base, live: cache::Live) -> Self {
        // The same calls in the same order as `new` makes, so the map is the same.
        let mut name_map = FamilyNameMap::default();
        for names in &base.names {
            let mut names = names.iter();
            let Some(first) = names.next() else {
                continue;
            };
            let first_name_id = name_map.get_or_insert(first).id();
            for name in names {
                name_map.add_alias(first_name_id, name);
            }
        }
        let mut generic_families = GenericFamilyMap::default();
        for ((generic_family, _), names) in GENERIC_FAMILY_NAMES.iter().zip(&base.generics) {
            generic_families.append(
                *generic_family,
                names
                    .iter()
                    .filter_map(|name| name_map.get(name).map(FamilyName::id)),
            );
        }
        Self {
            name_map: Arc::new(name_map),
            generic_families: Arc::new(generic_families),
            source_cache: SourcePathMap::default(),
            family_map: HashMap::default(),
            config: None,
            script_charsets: ScriptCharSetMap::default(),
            cache: Some(live),
            config_tried: false,
        }
    }

    // ZGUI-PATCH: loads the configuration the first time a question needs it. Only a collection
    // built from the cache defers it; any other collection without one has no library or config.
    fn ensure_config(&mut self) -> bool {
        if self.config.is_none() && self.cache.is_some() && !self.config_tried {
            self.config_tried = true;
            self.config = Config::load();
        }
        self.config.is_some()
    }

    pub(crate) fn family(&mut self, id: FamilyId) -> Option<FamilyInfo> {
        match self.family_map.get(&id) {
            Some(Some(family)) => return Some(family.clone()),
            Some(None) => return None,
            None => {}
        }

        let family = self.family_uncached(id);
        self.family_map.insert(id, family.clone());
        family
    }

    pub(crate) fn fallback(&mut self, key: impl Into<FallbackKey>) -> Option<FamilyId> {
        let key: FallbackKey = key.into();
        // ZGUI-PATCH: the cache answers a question an earlier process asked. Otherwise the answer
        // goes to the cache, unless the question failed for a reason that can change.
        let script = key.script().to_bytes();
        let family_name = match self
            .cache
            .as_ref()
            .and_then(|cache| cache.fallback(script, key.locale_str()))
        {
            Some(name) => name.map(String::from),
            None => {
                let name = self.fallback_uncached(&key)?;
                if let Some(cache) = &mut self.cache {
                    cache.add_fallback(script, key.locale_str(), name.as_deref());
                }
                name
            }
        };
        self.name_map.get(&family_name?).map(FamilyName::id)
    }

    // ZGUI-PATCH: the fontconfig half of `fallback`. `None` when the question failed for a
    // reason that can change; `Some(None)` when fontconfig has no match.
    fn fallback_uncached(&mut self, key: &FallbackKey) -> Option<Option<String>> {
        if !self.ensure_config() {
            return None;
        }
        let config = self.config.as_ref()?;

        let mut pattern = Pattern::new()?;

        let locale_lang_set = key.locale_str().and_then(|locale| {
            let mut lang_set = LangSet::new()?;
            lang_set.add(CString::new(locale).ok()?.as_c_str());
            Some(lang_set)
        });
        let script_char_set = self.script_charsets.charset_for_script(key.script());

        if let Some(set) = locale_lang_set {
            pattern.add_langset(FC_LANG, &set);
        }
        if let Some(set) = script_char_set {
            pattern.add_charset(FC_CHARSET, set);
        }

        config.substitute(&mut pattern, FcMatchPattern);

        // This calls FcFontRenderPrepare for us.
        let font = match config.font_match(&pattern) {
            Ok(font) => font,
            Err(MatchErr::NoMatch) => return Some(None),
            Err(_) => return None,
        };

        Some(font.get_string(FC_FAMILY, 0).ok().map(String::from))
    }
}

impl SystemFonts {
    fn family_uncached(&mut self, id: FamilyId) -> Option<FamilyInfo> {
        let name = self.name_map.get_by_id(id).cloned()?;

        // ZGUI-PATCH: the faces come from the cache when an earlier process resolved this
        // family. Otherwise fontconfig resolves it and the cache keeps the faces.
        let faces = match self
            .cache
            .as_ref()
            .and_then(|cache| cache.family(name.name()))
        {
            Some(faces) => faces.to_vec(),
            None => {
                let faces = self.family_faces(&name)?;
                if let Some(cache) = &mut self.cache {
                    cache.add_family(name.name(), &faces);
                }
                faces
            }
        };

        let mut font_infos = SmallVec::<[FontInfo; 4]>::new();
        for face in faces {
            if let Some(font_info) = (|| {
                let source_info = self.source_cache.get_or_insert(Path::new(&face.path));

                let weight = face
                    .weight
                    .map(FontWeight::from_fontconfig)
                    .unwrap_or_default();
                let width = face
                    .width
                    .map(FontWidth::from_fontconfig)
                    .unwrap_or_default();
                let style = face
                    .slant
                    .map(FontStyle::from_fontconfig)
                    .unwrap_or_default();

                let mut font_info = FontInfo::from_source(source_info, face.index)?;
                // TODO(valadaptive): does this do anything anymore?
                font_info.maybe_override_attributes(width, style, weight);
                Some(font_info)
            })() {
                font_infos.push(font_info);
            }
        }

        if font_infos.is_empty() {
            return None;
        }

        Some(FamilyInfo::new(name.clone(), font_infos))
    }

    // ZGUI-PATCH: the fontconfig half of `family_uncached`: the faces of one family, before any
    // font file is read.
    fn family_faces(&mut self, name: &FamilyName) -> Option<Vec<cache::RawFace>> {
        if !self.ensure_config() {
            return None;
        }
        let config = self.config.as_ref()?;

        // Match by family name.
        let mut pattern = Pattern::new()?;
        pattern.add_string(FC_FAMILY, CString::new(name.name()).ok()?.as_c_str());
        config.substitute(&mut pattern, FcMatchPattern);

        let fc_fonts = config.font_sort(&pattern, false).ok()?;
        let mut faces = Vec::new();
        for font in fc_fonts.iter() {
            let Some(font) = config.font_render_prepare(&pattern, &font) else {
                continue;
            };
            let Ok(family_name) = font.get_string(FC_FAMILY, 0) else {
                continue;
            };
            // We've performed font substitution and then sorted everything by
            // "closeness", so the good fonts should be at the top. Once we see
            // a fallback font (one that's not part of the family we explicitly
            // asked for), we can stop.
            if family_name != name.name() {
                break;
            }

            let Ok(path) = font.get_c_string(FC_FILE, 0) else {
                continue;
            };
            // This part is Unix-specific. Sorry, Windows fontconfig user.
            faces.push(cache::RawFace {
                path: OsStr::from_bytes(path.to_bytes()).to_owned(),
                index: font.get_int(FC_INDEX, 0).map_or(0, |idx| idx.max(0) as u32),
                weight: font.get_int(FC_WEIGHT, 0).ok(),
                width: font.get_int(FC_WIDTH, 0).ok(),
                slant: font.get_int(FC_SLANT, 0).ok(),
            });
        }
        Some(faces)
    }
}

// ZGUI-PATCH: the library version, part of the cache key.
fn fontconfig_version() -> i32 {
    unsafe { ffi_dispatch!(feature = "fontconfig-dlopen", LIB, FcGetVersion,) }
}

const GENERIC_FAMILY_NAMES: &[(GenericFamily, &CStr)] = &[
    (GenericFamily::Serif, c"serif"),
    (GenericFamily::SansSerif, c"sans-serif"),
    (GenericFamily::Monospace, c"monospace"),
    (GenericFamily::Cursive, c"cursive"),
    (GenericFamily::Fantasy, c"fantasy"),
    (GenericFamily::SystemUi, c"system-ui"),
    (GenericFamily::Emoji, c"emoji"),
    (GenericFamily::Math, c"math"),
];
