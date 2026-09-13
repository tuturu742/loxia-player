# 11-07 · About view

**Phase:** 11 — Settings · **Agent:** E · **Size:** S
**Prerequisites:** `11-01`
**Reference:** `docs/11-packaging.md` §7

## Goal
The About section, showing version, environment, and the third-party licence notices. This satisfies
a **licence compliance requirement**, not just a nicety.

## Files
- `crates/loxia-tui/src/views/settings.rs` (extend)
- `THIRD_PARTY_LICENSES.md`

## Specification

```
  loxia 0.1.0
  A terminal music client for Emby

  Licence      GPL-3.0-or-later
  Build        1.97.1 / x86_64-unknown-linux-gnu / 2026-07-21
  libmpv       2.2.0 (loaded from /usr/lib/libmpv.so.2)
  Terminal     kitty · graphics: kitty · 204x52
  Config       ~/.config/loxia-player/config.toml
  Cache        ~/.cache/loxia-player  (2.1 GB / 5.0 GB)
  Downloads    ~/.local/share/loxia-player/downloads  (14.7 GB, 312 tracks)

  [l] Third-party licences   [d] Copy diagnostics
```

**`THIRD_PARTY_LICENSES.md`** is authored in this task and contains the mpv notice **verbatim** from
`design_overview` §10.2 — the copyright line, the source URL
(`https://github.com/mpv-player/mpv`), and the no-warranty disclaimer — plus a generated list of
Rust dependencies with their licences, produced by `cargo about` or `cargo-deny list`.

It is embedded with `include_str!` and rendered by `[l]` in a scrollable pane. Embedding rather than
reading from disk means the shipped notice can never drift from the shipped binary — which is
precisely the compliance requirement in `docs/11-packaging.md` §7 item 6.

**`[d] Copy diagnostics`** puts the whole About block, plus the keybinding conflict count and the
active config's non-secret values, on the clipboard for bug reports. It **must redact** the access
token and any custom header values.

The libmpv version and load path come from the engine; with `--no-audio` they read
`not loaded (--no-audio)`.

## Acceptance
- `about_shows_version_and_licence`
- `about_shows_libmpv_version_and_path`
- `about_shows_cache_and_download_sizes`
- `third_party_licenses_contains_mpv_notice_verbatim` — asserts the exact copyright line, the source
  URL, and the disclaimer text.
- `third_party_licenses_is_embedded_not_read_from_disk`
- `licences_pane_scrolls`
- `diagnostics_redacts_token`
- `diagnostics_redacts_custom_header_values`
- `diagnostics_includes_conflict_count`
- `no_audio_shows_not_loaded`
- `about_snapshot`, `licences_snapshot`

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 11's exit criteria in
`docs/08-roadmap.md` are met.
