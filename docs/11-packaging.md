# Packaging and licensing

loxia is licensed under `GPL-3.0-or-later`.

## Current distribution surface

The workspace builds the `loxia-player` executable. The executable owns terminal setup, runtime
bootstrap, diagnostics, and workers. Playback links to libmpv through `libmpv2`; platforms require
a usable libmpv installation or packaged library.

Brand assets live in `assets/`. `logo.svg` is the full-colour mark and `logo-mono.svg` is the
single-colour small-size variant. Theme assets and factory equalizer presets are source-controlled
runtime assets.

## Third-party notices

[`../THIRD_PARTY_LICENSES.md`](../THIRD_PARTY_LICENSES.md) records bundled or linked third-party
licensing information and the locked Rust dependency licence report. Regenerate its dependency
section with:

```text
cargo deny list --format human --layout crate
```

when the lockfile changes.

## Release work

Packaging automation and platform installers are not part of the current implementation. Their
planned work is captured in [`ROADMAP.md`](ROADMAP.md).
