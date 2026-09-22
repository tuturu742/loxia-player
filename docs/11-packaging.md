# Packaging

loxia is licensed under `GPL-3.0-or-later`. Distribution packages provide the `loxia-player`
executable, bundled assets, and the runtime dependencies required by the target platform.

## Runtime requirements

Playback uses libmpv through `libmpv2`. On Linux, users install mpv from their distribution. On
macOS, Homebrew's `mpv` package provides it. Windows packages include `mpv-1.dll`; the runtime
diagnostic tells users to reinstall the Windows package when that library is missing.

The application loads bundled themes and equalizer presets from the workspace assets at build time
where applicable. The bundled themes are listed in [`02-data-model.md`](02-data-model.md).

## Licensing

`THIRD_PARTY_LICENSES.md` records libmpv and Rust dependency licensing information. The Rust
dependency section is generated with:

```sh
cargo deny list --format human --layout crate
```

Regenerate that section when the locked dependency graph changes.

## Branding

`assets/logo.svg` is the full-colour mark and `assets/logo-mono.svg` is the monochrome mark.
`assets/BRANDING.md` describes their intended use, palette, sizing, and clear space.

See [`13-dependencies.md`](13-dependencies.md) for dependency constraints and
[`ROADMAP.md`](ROADMAP.md) for outstanding distribution work.
