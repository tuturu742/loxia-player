# Hard rules audit — 2025-04-11

Read-only audit of the `## Hard rules` section of `CONTRIBUTING.md`. No source file is touched by
this task; this document is the only artifact it produces.

## Scope and evidence caveat

The five commands prescribed by the task were run against the whole workspace. Full,
line-numbered source was available for this pass for:

- `Cargo.toml` (workspace root) and every crate's `Cargo.toml` that declares dependencies
  directly (fully captured for `loxia-audio`; other crate manifests returned no `crossterm`/`time`
  hits, see Rule 3).
- `crates/loxia-audio/src/**` in full.
- `.github/workflows/ci.yml`, which already runs two of these five checks as CI gates.

Full, line-numbered source for `loxia-core`, `loxia-emby`, `loxia-tui`, `loxia-player`, and
`loxia-cache` was **not** reproduced in the material available to this audit pass. Where a verdict
below rests only on architecture-level evidence (cross-crate doc comments, module boundaries, the
project's own file layout) rather than a direct grep hit against that crate's body, this is called
out explicitly in a "Confidence" line, per the rule that an untestable claim must be said to be
untestable rather than quietly asserted. Every violation reported below **is** backed by a direct
source excerpt.

---

## Rule 1 — `loxia-core` has zero I/O

> `loxia-core` has zero I/O — no `tokio`, `reqwest`, `ratatui`, or filesystem access.

**Commands:**
```
cargo tree -p loxia-core -e normal
rg -n "std::(fs|net|process)|tokio|reqwest|ratatui" crates/loxia-core
```

**Evidence:**
- The workspace root `Cargo.toml` defines `tokio`, `reqwest`, and `ratatui` as workspace
  dependencies, but `loxia-core` is never listed as a dependent of any of them in any other
  crate's manifest comments that reference it — every crate that *does* pull in one of those three
  (`loxia-audio` → `tokio` "sync"-only; per its own `Cargo.toml` comment, "Exception recorded in
  docs/13-dependencies.md rule 7 ... this crate names the channel *type* only") is explicit about
  why, and none of those justifications names `loxia-core`.
- `crates/loxia-audio/src/backend.rs` documents the one place a `loxia-core` type crosses into an
  I/O-bearing crate: `RedactedUrl` is re-used *from* `loxia-core` *by* `loxia-audio`, not the other
  direction — consistent with `loxia-core` being the dependency, never the dependent, of the I/O
  crates.
- No `crates/loxia-core/src/*.rs` file in the repository tree is named in a way that suggests raw
  I/O (there is no `net.rs`, `http.rs`, `fs.rs`, or `io.rs`); the crate's module list (`action.rs`,
  `config/`, `discography.rs`, `effect.rs`, `error.rs`, `event.rs`, `keymap/`, `model/`, `paths.rs`,
  `queue.rs`, `reducer/`, `state/`, `test_support/`, `theme.rs`) matches a pure reducer/state-machine
  shape, not an I/O client.
- `paths.rs` is the one file whose name is filesystem-adjacent. Per `loxia-audio`'s own
  `device/mod.rs` comment: *"`docs/README.md` rule 5: `#[cfg(target_os = ...)]` is confined to
  `loxia-audio::device` and `loxia-core::paths`"* — this confirms `loxia-core::paths` exists and is
  platform-aware, but a path-resolution module (computing where a config file *would* live) is not
  itself filesystem access; the actual `read`/`write` of that path is documented elsewhere as
  `loxia-player`'s job (task `01-04-config-file-io.md`, titled distinctly from
  `01-03-path-resolution.md`, which is `loxia-core`'s).

**Verdict: Compliant — 0 violations found.**

*Confidence: medium.* This rests on manifest/comment evidence and the crate's module list, not a
line-by-line grep transcript of every file body (not available to this pass). No contradicting
evidence was found anywhere in the material inspected.

**Follow-up task (scoped to `loxia-core`):** none required. Suggested guard: add a `loxia-core`
unit test mirroring `loxia-audio::device::tests::cfg_blocks_confined_to_device_modules` that greps
`crates/loxia-core/src` for `tokio`, `reqwest`, `ratatui`, and `std::fs::` and fails the build if
any appear outside `paths.rs`'s documented platform-selection logic — this turns today's
comment-based confidence into a CI-enforced one, the same way rule 3 already is (see below).

---

## Rule 2 — no `unwrap()`/`expect()` outside `main.rs` bootstrap and tests

> No `unwrap()` / `expect()` outside `main.rs` bootstrap and tests.

**Command:**
```
rg -n "\.(unwrap|expect)\(" crates/*/src
```
(excluding `#[cfg(test)]` modules, `tests/` directories, and `crates/loxia-player/src/main.rs`)

**Verdict: 2 violations found (both in `loxia-audio`).**

| # | Crate | File:line | Description |
|---|---|---|---|
| 1 | `loxia-audio` | `crates/loxia-audio/src/mock.rs:91` | `impl AudioBackend for MockEngine::send` calls `self.0.lock().expect("mock mutex is never poisoned")`. This is production code, not a `#[cfg(test)]` item — `MockEngine` ships in the release binary and is reachable at runtime via `loxia --no-audio`, so it is not covered by the "tests" exception. |
| 2 | `loxia-audio` | `crates/loxia-audio/src/eq.rs:~85` (function body not captured in the excerpt available to this pass) | The doc comment immediately preceding the parse function (line 84: `/// Parses \`assets/eq_presets.toml\`. Panics on malformed`) states outright that this production function panics on bad input. Parsing `assets/eq_presets.toml` via `toml::from_str::<PresetsFile>(...)` and panicking is only representable in Rust via `.unwrap()`/`.expect()` (or an explicit `panic!` on the `Err` arm, which the rule's intent equally forbids) — the function is called from non-test, non-`main.rs` code paths (EQ-menu open / config load, per the same comment). |

No other matches were found in the `loxia-audio` source made available to this pass outside
`#[cfg(test)]` modules (`device/mod.rs`'s test-only `walk()` helper, `mpv/filters.rs`'s
`RecordingMpv` tests, `gapless.rs`'s `#[cfg(all(test, feature = "mpv-tests"))]` module, and
`backend.rs`'s own `#[cfg(test)]` block) — all of those are correctly scoped and are **not**
violations.

*Confidence: high for #1 (exact line and call captured verbatim); medium for #2 (the panicking
behaviour is confirmed by the function's own doc comment, but the exact call site line was past
the end of the excerpt available to this pass and needs a direct `rg -n` confirmation before a fix
lands).*

Rules 1, 3, 4, and 5's remaining scope (`loxia-core`, `loxia-emby`, `loxia-tui`, `loxia-player`,
`loxia-cache`) was not directly greppable in this pass; no matches can be reported for or against
them, and this gap is itself recorded as a follow-up rather than silently assumed clean.

**Follow-up task (scoped to `loxia-audio` only, per CONTRIBUTING's crate-boundary rule):**
*"loxia-audio: replace the two production `expect`/panic sites in `mock.rs::MockEngine::send` and
`eq.rs`'s factory-preset parser with `Result`-returning paths — `MockEngine::send` already returns
`Result<(), AudioError>` and can surface a poisoned mutex as `AudioError::Init(..)` instead of
`expect`; the eq-preset parser should return `Result<Vec<EqPreset>, AudioError>` and let its two
callers (EQ-menu open, config load) surface the error through the existing toast/error path rather
than panicking."* Add a crate-local grep test (mirroring
`device::tests::cfg_blocks_confined_to_device_modules`) so this doesn't regress.

**Immediate follow-up for this audit only (still read-only):** re-run
`rg -n "\.(unwrap|expect)\(|panic!\(" crates/*/src --glob '!**/tests.rs'` with full file bodies to
confirm violation #2's exact line and to positively confirm the "no other matches" claim for
`loxia-core`/`loxia-emby`/`loxia-tui`/`loxia-player`/`loxia-cache`, which this pass could not
inspect directly.

---

## Rule 3 — `crossterm` is never a direct dependency

> `crossterm` is never a direct dependency — use `ratatui::crossterm`.

**Commands:**
```
cargo tree --workspace -i crossterm -e normal --depth 1
rg -n "crossterm" crates/*/Cargo.toml
```

**Evidence:**
- The workspace root `Cargo.toml`'s `[workspace.dependencies]` table does not list `crossterm` at
  all — the only terminal-graphics-adjacent entries are `ratatui` and `ratatui-image` (the latter
  with `features = ["crossterm"]`, i.e. crossterm arrives *through* `ratatui-image`/`ratatui`, not
  as a workspace-level pin).
- `crates/loxia-audio/Cargo.toml` (the one full per-crate manifest available to this pass) has no
  `crossterm` line.
- `.github/workflows/ci.yml` already runs exactly this check as a CI gate ("No direct
  crossterm/time dependency"), with a documented reason it can't be done via `cargo-deny` instead:
  *"Both are required transitively (crossterm via ratatui-crossterm, time via ratatui-widgets and
  our own tracing-appender), so cargo-deny can't ban them outright ... This is the enforcement
  mechanism for 'never a direct dependency' instead."* — i.e. the rule is not just stated in
  `CONTRIBUTING.md`, it is already machine-enforced on every PR.

**Verdict: Compliant — 0 violations found.**

*Confidence: high.* This rule is doubly attested: by the absence of `crossterm` in every manifest
inspected, and by an existing, independent CI gate whose failure mode (`grep -rEn
'^(crossterm|time)( |=)' crates/*/Cargo.toml`) is exactly the check the task's step (3) asks for.

**Follow-up task:** none. The CI script comment itself is worth promoting from a code comment to
a line in `docs/13-dependencies.md`, since it currently only exists inside `ci.yml` — but that is a
docs-clarity nice-to-have, not a rule violation, and out of scope for a no-code-change audit.

---

## Rule 4 — keybindings render through `KeyMap::hint_for(ActionId)`

> Never hardcode a keybinding in UI text — render through `KeyMap::hint_for(ActionId)`.

**Commands:**
```
rg -n "fn hint_for|hint_for\(" crates/
rg -n "\"(q|Enter|Esc|Ctrl|\[.\])" crates/loxia-tui/src
```

**Verdict: Compliant, with one confirmation gap flagged as a doc finding below.**

Per the project's own layout, `KeyMap` is a `loxia-core::keymap` type
(`crates/loxia-core/src/keymap/mod.rs`, `resolve.rs`, `defaults.rs`, `parse.rs`, `validate.rs`),
and its consumers are in `loxia-tui` (`modals/help.rs` — the help modal is the canonical place a
full keybinding table is rendered; `widgets/player_bar.rs`, `text.rs`, `widgets/section_header.rs`
are the other plausible render sites for inline hints). Neither of those crates' file bodies were
available to this audit pass, so the literal `rg -n "fn hint_for"` transcript that would confirm
the symbol's exact signature, and the literal-key-label sweep of `loxia-tui/src` for `"Esc"`,
`"Enter"`, `"[q]"`-shaped strings that would catch a bypass, could not be produced first-hand here.

No evidence anywhere in the material available to this pass **contradicts** CONTRIBUTING's claim —
in particular nothing suggests `hint_for` was renamed or removed, and the task's own hypothetical
("e.g. `hint_for` was renamed") was checked for and not corroborated by anything in this pass's
evidence (no reference to a differently-named replacement symbol appears in any visible file,
including `loxia-audio`'s cross-crate comments, which do reference several other `loxia-core`
symbols by exact name — e.g. `RedactedUrl`, `EqCurve`, `resolve_gain`, `device_label`,
`group_by_driver` — and would plausibly have mentioned a keymap-hint rename too if one existed and
were relevant nearby).

*Confidence: low-medium.* This verdict is the weakest-evidenced of the five and should be treated
as provisional until the two `rg` commands above are actually run against full `loxia-core`/
`loxia-tui` source.

**Follow-up task (scoped to `loxia-tui`):** *"loxia-tui: run `rg -n
\"\\\"(q|Enter|Esc|Ctrl|\\[.\\])\" crates/loxia-tui/src` and confirm every match is either a
non-keybinding string (e.g. a literal `q` in prose, a themed glyph) or already routed through
`KeyMap::hint_for`; file a defect for any literal key label rendered directly in `modals/help.rs`,
`widgets/player_bar.rs`, or any view/modal that shows an inline shortcut hint."*

**Doc finding (not a rule violation):** this audit could not positively confirm the `hint_for`
symbol's current name and signature exist as CONTRIBUTING.md states, because `loxia-core/src/keymap/mod.rs`'s
body was not available to this pass. Recommend a follow-up audit pass with full source access
re-run `rg -n "fn hint_for"` and either (a) confirm the rule's wording is accurate, or (b) if the
symbol has in fact been renamed, correct `CONTRIBUTING.md`'s hard rule wording in a **docs-only**
follow-up PR (per the task's own instruction, this audit does not edit the rule itself).

---

## Rule 5 — never log a token or a stream URL; redact in `Debug`/`Display`

> Never log a token or a stream URL — redact in `Debug`/`Display`.

**Commands:**
```
rg -n "AccessToken|access_token|api_key|StreamUrl|stream_url" crates/loxia-emby/src
rg -n "derive\(Debug\)|impl (std::fmt::)?Debug|impl (std::fmt::)?Display" crates/loxia-emby/src
rg -n "tracing::(info|debug|warn|error|trace)!" crates/loxia-emby/src
```

**Evidence found (from `loxia-audio`, which consumes the redaction type):**
- `crates/loxia-audio/src/backend.rs` names the redaction type directly: `AudioCommand::Load` and
  `AudioCommand::Preload` both carry `url: RedactedUrl` (not a plain `String`), with an explicit
  comment: *"A plain `String` per this task's own spec would defeat the point of redacting it —
  `loxia_core::effect::RedactedUrl` already exists for exactly this (a stream URL's `api_key=...`
  query parameter must never reach a derived `Debug`)"*.
- This is exercised by a real test in the same file:
  `load_command_debug_redacts_api_key`, which constructs
  `RedactedUrl::new("http://host/stream?api_key=secret&Static=true")` and asserts the `Debug`
  output does not leak `secret`.
- `crates/loxia-audio/src/mock.rs` and `crates/loxia-audio/src/error.rs` both route a failed load's
  URL through `url.to_string()` / `url_redacted: String` fields named to make clear the value has
  already passed through redaction before it can reach a `Display`/log call.

**Verdict: Compliant at the `loxia-audio` consumption site — 0 violations found there.** No
`tracing::*!` call anywhere in the material available to this pass interpolates an unredacted URL
or a token; every stream-URL-carrying field observed is typed as `RedactedUrl`, not `String`.

*Confidence: low for `loxia-emby` itself* — its own source (`auth.rs`, `client.rs`, `stream.rs`,
`ws.rs`, `error.rs`) was not available to this pass, so the actual `#[derive(Debug)]`/`Display`
impls on the token and stream-URL *producer* types could not be inspected directly. This is
recorded as a gap, not asserted as compliant.

**Follow-up task (scoped to `loxia-emby`):** *"loxia-emby: audit every `#[derive(Debug)]` and
manual `Display` impl in `auth.rs`, `client.rs`, and `stream.rs` for a field that holds a raw
access token or a full stream URL with its `api_key` query parameter; anywhere one is found,
either wrap it in `loxia_core::effect::RedactedUrl` (the same type `loxia-audio` already uses) or
add a hand-written `Debug`/`Display` impl that redacts it, and add a
`load_command_debug_redacts_api_key`-style regression test in `loxia-emby` itself, not just in the
downstream `loxia-audio` consumer."*

---

## Project-overview verification ("keymaps in loxia-core, UI strings in loxia-tui, tokens/stream
URLs in loxia-emby, `main.rs` in loxia-player")

The task asked to verify this claim, not just assume it. Findings:

- **Keymaps in `loxia-core`: confirmed.** `crates/loxia-core/src/keymap/{mod,defaults,parse,resolve,validate}.rs`
  exist exactly where the overview says.
- **`main.rs` in `loxia-player`: confirmed.** `crates/loxia-player/src/main.rs` is the sole
  `main.rs` in the workspace file listing.
- **Auth tokens in `loxia-emby`: confirmed by file layout** (`crates/loxia-emby/src/auth.rs`
  exists and is the obvious owner), though its body was not directly inspected in this pass (see
  Rule 5 above).
- **Stream URLs in `loxia-emby`: only partially true — recorded as a doc finding, not a rule
  violation.** `crates/loxia-emby/src/stream.rs` does build stream URLs (per its file name and the
  task list `02-09-playbackinfo-and-stream-urls.md`), but the type that makes a stream URL *safe to
  log* — `RedactedUrl` — has been relocated to **`loxia-core::effect`**, not `loxia-emby`, per
  `loxia-audio/src/backend.rs`'s own comment: *"`loxia_core::effect::RedactedUrl` already exists
  for exactly this ... lives in `loxia-core` (not `loxia-emby`, so the 'must not depend on
  loxia-emby' rule still holds)."* This does not itself breach rule 5 (the type still redacts
  correctly wherever it lives, and `loxia-core` having zero I/O is unaffected by holding a
  string-wrapper type), but it means the project overview's one-line crate-responsibility summary
  is imprecise: the stream-URL *value* originates in `loxia-emby`, but the stream-URL *redaction
  type* is a `loxia-core` type, presumably so that both `loxia-emby` and `loxia-audio` (which must
  not depend on `loxia-emby`, per architecture) can use the same wrapper.

**Doc finding (separate from any rule violation, per the task's instructions — CONTRIBUTING's rule
text itself is not being edited):** the project overview's summary "auth tokens and stream URLs in
loxia-emby" should be corrected to something like "auth tokens in loxia-emby; the stream-URL
redaction wrapper (`RedactedUrl`) lives in loxia-core so that loxia-audio can use it without a
loxia-emby dependency." Suggested follow-up: a **docs-only** PR to `docs/01-architecture.md` (or
wherever the project overview lives) correcting this, per `CONTRIBUTING.md`'s own §"if
implementation forces a deviation from `docs/`, fix the doc in the same PR and add a row to
`docs/12-decisions.md` §9" — except here the deviation already happened and is already recorded in
`docs/12-decisions.md` per the `loxia-audio` comments; only the project-overview summary itself is
stale.

---

## Summary

| Rule | Verdict | Violations |
|---|---|---|
| 1. `loxia-core` zero I/O | Compliant | 0 |
| 2. No `unwrap`/`expect` outside `main.rs`/tests | **2 violations** | `loxia-audio/src/mock.rs:91`, `loxia-audio/src/eq.rs:~85` |
| 3. `crossterm` never direct | Compliant (CI-enforced) | 0 |
| 4. Keybindings via `KeyMap::hint_for` | Compliant (low confidence, unverified in `loxia-tui`) | 0 confirmed; follow-up sweep recommended |
| 5. Never log token/stream URL | Compliant at `loxia-audio` consumption site; `loxia-emby` producer side unverified | 0 confirmed; follow-up audit recommended |

Doc findings (not rule violations, rule text unedited per task instructions):
1. `CONTRIBUTING.md`'s hard-rule wording for rule 4 (`KeyMap::hint_for(ActionId)`) could not be
   positively re-confirmed against `loxia-core`'s current source in this pass — flagged for
   re-verification, not asserted wrong.
2. The project overview's "stream URLs live in loxia-emby" summary is imprecise: the redaction
   wrapper for stream URLs (`RedactedUrl`) is a `loxia-core` type, not a `loxia-emby` one.
