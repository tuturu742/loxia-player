# Packaging and diagnostics

The distributable application is the `loxia-player` binary package. It requires a usable libmpv
installation or platform bundle appropriate to the target.

## Runtime diagnostics

The `doctor` module in `loxia-player` reports environment and dependency information used to
diagnose startup and media-library problems. Audio-library failures include platform-appropriate
installation guidance.

Logs and diagnostics avoid access tokens and stream URLs. Configuration and cache locations come
from the platform path helpers rather than the repository checkout.

## Assets and licensing

`assets/` contains the bundled themes, equalizer presets, and branding assets. `assets/BRANDING.md`
describes the existing logo assets and their intended use.

The workspace license is GPL-3.0-or-later. `THIRD_PARTY_LICENSES.md` records third-party licensing
information for the locked dependency graph and bundled libmpv relationship. Dependency additions
follow `13-dependencies.md` and the repository's deny policy.

## Release verification

Packaging work uses the automated checks in `10-testing-and-ci.md` and the manual scenarios in
`14-manual-test-plan.md`. Platform packaging details that are not implemented remain in
`ROADMAP.md`.
