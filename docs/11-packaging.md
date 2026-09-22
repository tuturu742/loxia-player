# Packaging and distribution

loxia is licensed under GPL-3.0-or-later. The workspace package metadata lives
in the root `Cargo.toml`; `loxia-player` is the executable composed from the
workspace crates.

## Runtime dependency

The production audio backend dynamically uses libmpv. On systems where libmpv
is not available, the audio layer reports a platform-specific installation
hint. The audio build script probes `pkg-config` for mpv and adds discovered
library search paths and rpaths.

The application does not require libmpv for unit tests that use the mock audio
backend. Tests that instantiate a real mpv engine are gated behind the
`mpv-tests` feature.

## Assets

The distribution includes the SVG branding assets, shipped themes, and factory
equalizer presets in `assets`. `assets/BRANDING.md` defines the mark's intended
uses and size constraints.

## Licence records

`THIRD_PARTY_LICENSES.md` records third-party licence information for the
locked dependency graph and libmpv. Regenerate its dependency section with:

```sh
cargo deny list --format human --layout crate
```

The file reflects `Cargo.lock`; dependency changes require corresponding
licence review and updates to [`13-dependencies.md`](13-dependencies.md).
