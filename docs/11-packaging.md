# Packaging and diagnostics

loxia is distributed as the `loxia-player` binary. The workspace package metadata identifies the
project as GPL-3.0-or-later and records its repository and Rust version requirements.

## Runtime dependency

The production audio backend dynamically links libmpv through `libmpv2`. Linux distributions
normally provide it with their mpv package; macOS users can install it with Homebrew; and the
Windows installer supplies `mpv-1.dll`. Startup diagnostics report a platform-specific installation
hint when the library is absent.

## Assets

Bundled themes live in `assets/themes`. Factory equalizer presets live in
`assets/eq_presets.toml`. Branding assets live in `assets/logo.svg` and
`assets/logo-mono.svg`; their visual-use guidance is in `assets/BRANDING.md`.

## Licensing

`LICENSE` contains the project licence. `THIRD_PARTY_LICENSES.md` records linked and bundled
third-party licences from the locked dependency graph. It is regenerated with:

```text
cargo deny list --format human --layout crate
```

The dependency rules that support packaging and licence review are in
[`13-dependencies.md`](13-dependencies.md).

## Diagnostics

`loxia-player` includes bootstrap and doctor support for environment diagnostics. Runtime logs
avoid tokens and stream URLs. Audio, network, and cache errors expose stable user-facing messages
while preserving structured details for diagnostics.
