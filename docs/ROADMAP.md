# Roadmap

This document records work that is not part of the current implementation. The reference documents
describe existing behaviour; an item below becomes reference documentation only when the code
implements it.

## Distribution and release delivery

- Produce and maintain platform installers and release automation for Linux, macOS, and Windows.
- Generate and package platform icon bundles from the branding SVGs.
- Add the ASCII startup banner and wire version substitution through the application.
- Publish user-facing installation and operational documentation with released artifacts.

## Playback and audio

- Add bit-perfect output support when a portable implementation and test strategy are available.
- Revisit live equalizer-band updates if a supported mpv/FFmpeg route can update the
  `lavfi`-wrapped `anequalizer` graph without replacing the filter property.
- Add additional quality-profile controls only when they map to supported Emby playback settings.

## Integrations

- Add remote-control support only with an authenticated protocol and a maintained compatibility
  story.
- Extend desktop integration beyond the currently implemented media-key and notification paths
  where platform support is reliable and testable.

## Documentation maintenance

- Keep this list limited to work that is not implemented.
- Move an item into the relevant reference document when implementation, tests, and operational
  constraints exist; record behaviour-changing documentation corrections in
  [`12-decisions.md`](12-decisions.md).
