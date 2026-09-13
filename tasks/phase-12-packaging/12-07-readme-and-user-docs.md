# 12-07 · README and user docs

**Phase:** 12 — Packaging · **Agent:** E · **Size:** M
**Prerequisites:** `11-07`, `12-06`
**Reference:** `docs/11-packaging.md`, `docs/04-state-and-input.md` §6

## Goal
User-facing documentation: the README, the keybinding reference, the config reference, and
troubleshooting. Everything a user needs that is not in the app itself.

## Files
- `README.md`
- `docs/user/configuration.md`
- `docs/user/keybindings.md`
- `docs/user/troubleshooting.md`

## Specification

**README** sections, in this order:
1. One-sentence description, the logo, and a screenshot or asciinema recording of the Miller view.
2. Features — honest and short. **Do not list spectrum analysis, crossfade, or star ratings**; they
   are not in the product (`docs/12-decisions.md` §§2–3).
3. Install, per platform, with the mpv requirement stated plainly for macOS and Linux and the
   bundled DLL noted for Windows.
4. First run: creating the config, connecting to a server, and the custom-headers example for
   reverse-proxy users.
5. A keybinding summary table for the most common two dozen keys, linking to the full reference.
6. Configuration summary, linking to the full reference.
7. Troubleshooting links, and `loxia-player --doctor` as the first thing to run.
8. Licence: GPL-3.0-or-later, with the mpv/LGPL note and the source pointer.

**`docs/user/keybindings.md`** is the complete table from `docs/04-state-and-input.md` §6, in
user-facing language, plus how to remap in Settings and in `config.toml`, and an explanation of what
happens on a conflict. **Generate it from `KeyMap::defaults()`** with a small `xtask` or an
`#[ignore]`d test that writes the file, and add a CI check that the committed file matches the
generated one — hand-maintained key documentation drifts within two releases.

**`docs/user/configuration.md`** documents every `config.toml` key: type, default, effect, and any
interaction with others (bit-perfect disabling EQ, `download_uncompressed` overriding the quality
profile). Also generated and CI-checked against the schema.

**`docs/user/troubleshooting.md`** covers, at minimum: libmpv not found on each platform; no audio
output; bit-perfect failing to engage; artwork not rendering (protocol support); `Alt+N` swallowed
by the terminal or tmux, with the `F1`–`F9` alternative; a token rejected by a reverse proxy, with
the custom-headers fix; Gatekeeper on unsigned macOS builds; and where log files live.

Screenshots and recordings live in `assets/screenshots/`. Record with a widely available font, in
the `cyberpunk_neon` and `default_terminal` themes.

## Acceptance
- The README covers all eight sections and mentions none of the cut features.
- `docs/user/keybindings.md` matches `KeyMap::defaults()` — CI-checked.
- `docs/user/configuration.md` documents every field in `Config` — CI-checked.
- Troubleshooting covers all eight listed scenarios.
- Every install command in the README has been executed successfully on its platform.
- Screenshots render in the GitHub UI and are under 500 KB each.
- No document mentions spectrum analysis, crossfade, or star ratings.

## Done when
The global DoD in `tasks/README.md` is satisfied.
