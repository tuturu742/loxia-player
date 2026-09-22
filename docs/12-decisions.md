# Decisions

This document records decisions that define the current implementation or correct previously
documented behaviour. New behaviour-changing documentation corrections belong in §9.

## 1. Core remains I/O-free

`loxia-core` contains domain types, state, reducers, configuration logic, and pure helpers. It does
not depend on HTTP, terminal rendering, audio backends, or filesystem operations.

## 2. The binary crate owns integration

`loxia-player` owns runtime assembly, dispatch, terminal lifecycle, and workers. This keeps
integration dependencies out of `loxia-core` and `loxia-tui`.

## 3. Sensitive transport values stay redacted

Access tokens and stream URLs do not appear in ordinary diagnostics, error messages, or fixtures.
`RedactedUrl` provides a boundary type for stream URLs passed to audio code.

## 4. Key hints come from the keymap

UI text resolves action hints through `KeyMap::hint_for(ActionId)` rather than embedding default
keys. Customized bindings and the help modal therefore share one source.

## 5. Equalizer application uses the mpv `af` property

The mpv backend applies the `lavfi`-wrapped `anequalizer` filter chain by resetting `af`. This is
the working libmpv path used by the implementation; the pure band-command helper remains available
for testing and future compatibility work.

## 6. Platform strings do not require platform-specific branches

Audio installation hints select strings using `std::env::consts::OS`. This supplies target-specific
guidance without spreading `#[cfg(target_os)]` branches through audio logic.

## 7. Cache work stays outside reducers

Reducers emit effects for cache and persistence work. The cache crate performs filesystem work and
returns results through normal application events.

## 8. Themes are data files

Bundled themes live in `assets/themes/*.toml` and are parsed through the core theme layer. CRT
themes explicitly request ASCII-only output.

## 9. Documentation corrections

| Correction | Current behaviour | Reason |
| --- | --- | --- |
| Binary crate name and location | The binary package is `loxia-player` in `crates/loxia-player`; references to `loxia` or `crates/loxia` do not describe this workspace. | The Cargo workspace and source tree use `loxia-player`. |
| Repository-root configuration | A repository-root `config.toml` is ignored and is not a supported configuration location. | User configuration resolves through platform paths, and the ignore rule protects accidental local credentials. |
| Unimplemented visual audio features | Crossfade and a spectrum analyser are not described as current audio behaviour. | They are planned work recorded in `ROADMAP.md`. |

## 10. Documentation scope

The numbered documents describe current behaviour and source boundaries. Forward-looking work is
kept in `ROADMAP.md` so reference documents remain useful while the implementation changes.
