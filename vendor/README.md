# Vendored crates

## fontique 0.11.1 (crates.io)

The font collection under parley, patched in through `[patch.crates-io]` so that parley resolves
against the same copy. Without the examples. Every local change is marked `ZGUI-PATCH:` in the
source. The source of 0.11.1 is the same as that of 0.11.0 (upstream commit 033e0b0); only its
manifest differs, and it takes read-fonts 0.41.

### Published crates

`[patch.crates-io]` applies only to a build of this workspace. An application that depends on
zgui from crates.io builds against the released fontique: it gets the same fonts and resolves them
more slowly. It can use this copy with the same patch, because cargo finds a package anywhere in a
git repository:

```toml
[patch.crates-io]
fontique = { git = "https://github.com/zortax/zgui", tag = "v0.1.0" }
```

### Query state kept across queries (`collection/query.rs`)

Upstream clears a collection's query state when a query starts and again when it ends, and parley
starts one query per paragraph. For a generic family such as `sans-serif`, which a system
collection resolves to some two hundred families, every paragraph then rebuilt that list, and
every digit, which parley shapes as a possible emoji, rebuilt it again with the emoji families
appended. The state now outlives the query:

- the current family list and the last three are kept with the families and faces they loaded,
  and a `set_families` call that repeats one of them reuses it;
- the fallback list is rebuilt only when its script and locale change;
- loaded faces are dropped in `matches_with` when the attributes changed since they were matched;
- a query that sets no families or fallbacks still matches none.

`Collection` counts a generation, raised by every mutating method and by a sync that copies a
shared clone's changes; a query that finds the state built against another generation clears it.
`crates/zgui-text-parley/tests/fonts.rs`
(`a_face_unregistered_after_shaping_is_not_drawn_with_again`) pins the invalidation.

### System font cache (`backend/fontconfig.rs`, `backend/fontconfig_cache.rs`, `system_cache.rs`)

On Linux, a system collection spent about 27 ms of a cold start in fontconfig: it loaded the
configuration and sorted all installed faces once for each of the eight generic families. Each
family a document named then cost one more sort of all faces (about 1.5 ms each). The backend now
keeps the results in a file:

- the file holds the family names, the generic family lists, and each family and fallback that a
  process resolved (a record is appended when a process resolves a new one);
- a process that finds the file valid builds the collection from it and loads fontconfig only for
  a question the file cannot answer;
- the file is valid while every font directory and configuration file that fontconfig reported (and
  the directories that hold the configuration files) has the same identity, size and modification
  time, and while the fontconfig version and the environment variables that fontconfig reads are
  the same; a font install or removal changes its directory, so the next process writes a new file;
- a stamp that changed after the scan started, or less than two seconds before it, can hold a
  change the scan did not see, so no file is written then;
- the file is written again only when a stamp changes, so processes that run at the same time can
  append the same record twice;
- `ZGUI_FONT_CACHE=off` disables the file, `ZGUI_FONT_CACHE=<absolute dir>` moves it, and the
  default is `$XDG_CACHE_HOME/zgui` or `$HOME/.cache/zgui`.

`fontique::system_cache_counts` reports hits, misses, writes and configuration loads.
`crates/zgui-text-parley/tests/font_cache.rs` compares names, generic lists, families and fallbacks
with the cache off, on a miss and on a hit, and pins the invalidation on install and removal.

### Script samples (`script.rs`)

The sample table builds each `Script` with `Script::from_bytes`. Upstream calls
`from_str_unchecked`, which parlance 0.1.1 deprecates, and every build that compiles this copy as a
path dependency printed 161 warnings.
