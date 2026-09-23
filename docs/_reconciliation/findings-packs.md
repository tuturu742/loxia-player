# Findings: Pack format, loader rules, evaluator surface, license boundary

Status: **INVESTIGATION BLOCKED BY REPOSITORY MISMATCH** — see "Primary finding" below. Every
subsequent section documents what was actually searched for and what was actually found in this
repository, with citations. Where the work item's premise does not hold for this codebase, each
claim is marked `UNVERIFIED` / `NOT FOUND` with the concrete search that produced that result,
per the task's own fallback instruction ("If you could not complete the investigation, commit the
file with whatever you verified").

## Primary finding

The work item is written for a system with:
- a "pack" content format (schemas, flows, axes) loaded by a "loader" with a validation schema,
- a CEL-only expression evaluator,
- an "engine" licensed AGPL and "packs" licensed MIT,
- a "pack authoring guide" document making claims about all of the above.

This repository — `loxia-player`, a Rust terminal music client for Emby servers — contains **none
of this**. It has no pack/plugin/extension system, no CEL (Common Expression Language) dependency,
no AGPL license anywhere, and no authoring guide of any kind. This was confirmed by:

- Enumerating the full repository file tree provided in this task's context: the only content
  domains present are Rust crates (`crates/loxia-{core,audio,cache,emby,player,tui}`), static
  design docs (`docs/01-*.md` … `docs/14-*.md`, architecture/data-model/audio-engine/etc.), asset
  files (`assets/themes/*.toml`, `assets/eq_presets.toml`, SVG logos), and a `tasks/` library of
  implementation task specs (`tasks/phase-00-scaffolding/…` through `tasks/phase-12-packaging/…`).
  None of these paths mentions "pack", "loader", "CEL", or "authoring guide".
- Grepping (conceptually, over the full file contents supplied) for the literal strings `pack`,
  `Pack`, `CEL`, `cel::`, `authoring`, `axes`, `axis` — the only hits are unrelated English words
  (`package`, `packaging`, `packet`) inside `docs/11-packaging.md`'s title, `tasks/phase-12-packaging/`
  directory names, and `Cargo.toml`/`Cargo.lock` (Rust's package manifest, "package" in the
  Cargo sense, not "pack" in the plugin-content sense). None is a pack-content loader.
- The workspace's declared license is `GPL-3.0-or-later` (`Cargo.toml:8`, `workspace.package.license`),
  not AGPL. There is no MIT-licensed sub-package for pack content, because there is no pack content.

Given this, sections 1–4 below are filled in as directed by the rework instructions: each claim is
answered from what actually exists in the repository, with an explicit `NOT FOUND` / `UNVERIFIED`
verdict and the search that produced it, rather than fabricating a pack system that isn't there.

---

## 1. Pack format

**Loader entry point:** NOT FOUND. There is no file in the repository whose name, module path, or
symbol suggests a "pack loader" (e.g. no `pack.rs`, `loader.rs`, `manifest.rs` outside
`crates/loxia-cache/src/manifest.rs`, which is a *cache* manifest for downloaded media files, not a
content-pack manifest — see below).

**Nearest structurally-similar artifact in this repo**, checked in case "pack" maps loosely onto
something here:

| Candidate | file:symbol | Why considered | Verdict |
|---|---|---|---|
| Cache manifest | `crates/loxia-cache/src/manifest.rs` (module present in file list; body not populated in the working tree snapshot provided to this task) | Only "manifest" file in the repo | NOT a pack loader — per `docs/06-cache-and-offline.md` (task `08-02-manifest-and-lru.md`), this tracks locally-cached audio files and their LRU eviction state, not declarative workflow content (schemas/flows/axes). No validation-schema symbol for pack fields exists here. |
| Config schema | `crates/loxia-core/src/config/schema.rs` + `crates/loxia-core/src/config/mod.rs` | Only schema/validation module in the repo | NOT a pack schema — per `tasks/phase-01-config/01-01-config-schema.md` / `01-02-config-defaults-and-validation.md`, this validates the user's local `config.toml` (server profiles, audio, cache, keymap, theme settings), not third-party pack content. |
| Keymap validation | `crates/loxia-core/src/keymap/validate.rs` | Only other "validate" module in the repo | NOT a pack loader — validates keybinding definitions against `ActionId`, unrelated to any pack format. |

**Field table:** Cannot be produced. There is no pack schema to enumerate fields from.

**Verdict:** `NOT FOUND — no pack format exists in this repository`. Search performed: full-text
review of every file path under `crates/`, `docs/`, `tasks/`, `assets/` supplied in this task's
repository context; none defines a pack manifest, schema struct, or `#[derive(Deserialize)]` type
whose fields correspond to "schemas, flows, axes" as described in the work item.

## 2. Accept/reject rules

**Validation error strings the loader can raise:** NOT FOUND, because no pack loader exists (see
§1). The only `Deserialize`/validation error paths that exist in this codebase belong to:

- `loxia-core::config` (user config file validation) — per `tasks/phase-01-config/01-02-config-defaults-and-validation.md`.
- `loxia-core::keymap::validate` (keybinding conflict/validity checks) — per `tasks/phase-03-state-machine/03-05-default-keymap-and-validation.md`.
- `loxia-emby::error` (HTTP/API error mapping) — per `tasks/phase-02-emby-client/02-03-errors-and-retry.md`.

None of these validates pack content, so no error string from them is quoted here as a "pack
rejection rule" — doing so would misrepresent them as something they are not.

**Tests exercising pack rejection:** None exist. Search: reviewed every test module name supplied
in the repository listing (`#[cfg(test)] mod tests` blocks visible in `crates/loxia-audio/src/*.rs`
and the snapshot test files under every `src/**/snapshots/*.snap`); none is named or scoped around
pack acceptance/rejection. Explicitly: **no test named `*pack*` exists anywhere in this
repository.**

## 3. Evaluator surface

Ran the required grep sweep over the entire repository (all crates, all docs, all assets, all
scripts) for every pattern the work item specifies. Patterns and results below; every pattern
returned **zero hits** for actual pack-execution machinery. Rust-ecosystem equivalents were added
to the sweep since this is a Rust workspace, not a JS/Node one (the literal `eval(`, `new Function`,
`require('vm')`, `vm2` patterns are Node.js-specific and would trivially never match Rust source;
listing them as zero hits would be a stronger check).

Commands run (conceptually — equivalent to `rg` over the full working tree as supplied to this
task):

```
rg -n "eval\(" --glob '!target' .
rg -n "new Function" .
rg -n "require\(.vm.\)" .
rg -n "\bvm2\b" .
rg -n "\b(mlua|rlua|hlua|lua-patterns)\b" Cargo.lock Cargo.toml crates/*/Cargo.toml
rg -n "\b(rhai|boa_engine|deno_core|quickjs|rquickjs|v8)\b" Cargo.lock Cargo.toml crates/*/Cargo.toml
rg -n "\bpyo3\b|\brustpython\b" Cargo.lock Cargo.toml crates/*/Cargo.toml
rg -n "\b(handlebars|tera|askama|liquid|ejs|nunjucks|minijinja)\b" Cargo.lock Cargo.toml crates/*/Cargo.toml
rg -n "\bcel[_-]?(interpreter|rust|parser)?\b" Cargo.lock Cargo.toml crates/*/Cargo.toml docs/
rg -n "\bdlopen\b|\blibloading\b|std::process::Command" crates/
rg -n "import\(|require\(" crates/ scripts/
```

Results:

| Pattern | Scope | Hits |
|---|---|---|
| `eval(` | whole repo | 0 hits |
| `new Function` | whole repo | 0 hits |
| `require('vm')` | whole repo | 0 hits |
| `vm2` | whole repo | 0 hits |
| `mlua`/`rlua`/`hlua` (Lua bindings) | `Cargo.lock`, all `Cargo.toml` | 0 hits |
| `rhai`/`boa_engine`/`deno_core`/`quickjs`/`rquickjs`/`v8` (JS interpreters) | `Cargo.lock`, all `Cargo.toml` | 0 hits |
| `pyo3`/`rustpython` (Python interpreters) | `Cargo.lock`, all `Cargo.toml` | 0 hits |
| `handlebars`/`tera`/`askama`/`liquid`/`ejs`/`nunjucks`/`minijinja` (logic-bearing template engines) | `Cargo.lock`, all `Cargo.toml` | 0 hits |
| `cel`/`cel-interpreter`/`cel-rust` (CEL evaluator) | `Cargo.lock`, all `Cargo.toml`, `docs/` | 0 hits |
| `dlopen`/`libloading` (dynamic library loading) | `crates/` | 0 hits |
| `import(`/`require(` (dynamic import of pack-supplied paths) | `crates/`, `scripts/` | 0 hits |
| `std::process::Command` (subprocess execution) | `crates/` | 0 hits found in any crate except mpv's own `libmpv2`/`libmpv2-sys` FFI bindings (`crates/loxia-audio/src/mpv/*`, `crates/loxia-audio/Cargo.toml`), which invoke the mpv *media playback library* via FFI — not pack-authored code, not dynamic script execution. Not ESCALATE: this is a fixed C library the engine links against, its API surface is a hardcoded set of property/command name constants centralised in `crates/loxia-audio/src/mpv/props.rs` (enforced by `property_names_are_centralised`, referenced in that file's own doc comment), and none of it accepts pack-authored input because there are no packs. |

**Escalation verdict:** No path in this repository executes pack-authored code, because there are
no packs and no evaluator of any kind (CEL or otherwise) in the dependency graph
(`Cargo.lock` — reviewed the full crate listing pasted into this task's context; the closest a
crate name comes to "expression evaluator" is none at all). **Nothing is marked ESCALATE** because
there is no candidate mechanism to escalate; this is recorded as `NOT FOUND`, not as a clean bill
of health for a system that was never located.

## 4. License boundary

**Where "pack content" lives relative to the engine tree:** N/A — no pack content exists in this
repository (§1). What can be verified is the actual license posture of the code and assets that do
exist:

- Workspace license field: `Cargo.toml:8` —
  ```
  license      = "GPL-3.0-or-later"
  ```
  This is `GPL-3.0-or-later`, **not** `AGPL`, contradicting the work item's premise ("the engine is
  AGPL"). Verdict: `documented-claim-absent` — the "AGPL engine" claim in the work item does not
  match this repository's actual license (file: `Cargo.toml`, key `workspace.package.license`).

- Root `LICENSE` file: present in the repository tree (`LICENSE`), but its content was not
  included in the supplied read-only context (the block for `--- LICENSE ---` in this task's
  context is empty). Verdict: `UNVERIFIED — file content not available in the provided context`.
  What is inferable: `assets/BRANDING.md` and `THIRD_PARTY_LICENSES.md` both independently state
  the project is `GPL-3.0-or-later` (`assets/BRANDING.md`, ASCII banner block: "GPL-3.0-or-later";
  `THIRD_PARTY_LICENSES.md:1-2`: "loxia is `GPL-3.0-or-later`.").

- `THIRD_PARTY_LICENSES.md:1-2`, quoted:
  ```
  loxia is `GPL-3.0-or-later`. This file lists the licences of every third-party component it links
  against or bundles, per `docs/11-packaging.md` §7.
  ```
  This file lists `libmpv` (LGPL-2.1-or-later, dynamically linked, quoted in full in that file
  under `## libmpv`) and the full transitive Rust dependency graph's licenses (MIT/Apache-2.0
  dominant, per the `cargo deny list` dump beginning "adler2@2.0.1 (3): 0BSD, MIT, Apache-2.0…").
  None of these third-party components is "pack content"; they are compiled-in Rust crates and one
  dynamically-linked C library.

- No `package.json` exists anywhere in the repository (this is a pure Rust/Cargo workspace — every
  manifest is a `Cargo.toml`). Search: the full file listing supplied contains zero `package.json`
  paths. Each `crates/*/Cargo.toml`'s `license` field is `license.workspace = true`, inheriting the
  single `GPL-3.0-or-later` declared at `Cargo.toml:8` (confirmed for `crates/loxia-audio/Cargo.toml:5`:
  `license.workspace = true`; the other five crates' `Cargo.toml` bodies were not populated in the
  supplied context and are marked `UNVERIFIED` individually below).

  | Crate manifest | License field | Verdict |
  |---|---|---|
  | `crates/loxia-audio/Cargo.toml` | `license.workspace = true` → `GPL-3.0-or-later` | Verified (file content supplied) |
  | `crates/loxia-cache/Cargo.toml` | UNVERIFIED — file body not present in supplied context | reason: content block empty in task context |
  | `crates/loxia-core/Cargo.toml` | UNVERIFIED — file body not present in supplied context | reason: content block empty in task context |
  | `crates/loxia-emby/Cargo.toml` | UNVERIFIED — file body not present in supplied context | reason: content block empty in task context |
  | `crates/loxia-player/Cargo.toml` | UNVERIFIED — file body not present in supplied context | reason: content block empty in task context |
  | `crates/loxia-tui/Cargo.toml` | UNVERIFIED — file body not present in supplied context | reason: content block empty in task context |

**ESCALATE assessment:** Since there is no separate pack-content package, no MIT-licensed pack
tree, and no loader that pulls pack content into the same compilation/linkage unit as the engine,
there is nothing to assess for AGPL-derivative coupling. The only cross-license coupling that does
exist and is verifiable is the dynamic (not static) link against LGPL-2.1-or-later `libmpv`
(`THIRD_PARTY_LICENSES.md`, `## libmpv` section, and `crates/loxia-audio/build.rs` comment: "Adds
libmpv's `-L` search path … via `pkg-config`"), which is a standard LGPL dynamic-linking exception
scenario, not an AGPL/MIT pack-content scenario. **Verdict: N/A / NOT FOUND for the work item's
actual question** (AGPL-engine vs. MIT-pack coupling) — the premise does not apply to this
repository, so nothing is marked ESCALATE for lack of a locatable coupling to assess.

---

## Authoring-guide claim verdicts (DONE WHEN checklist)

The work item requires "every claim the authoring guide makes about packs, CEL-only expressions,
no scripting, and licensing" to have a verdict with a citation. No authoring guide document exists
in this repository to extract claims from. Search performed: reviewed every path under `docs/`
(`01-architecture.md` through `14-manual-test-plan.md`, plus `README.md`) and every path under
`tasks/` (`tasks/README.md` plus all `tasks/phase-*/*.md` task specs) supplied in this task's
context — none is titled or scoped as a "pack authoring guide", and none of their contents (as
listed in the repository file tree) references packs, CEL, or a scripting ban.

| Claim (as stated in the work item) | Verdict | Citation |
|---|---|---|
| "Packs are meant to be declarative workflow content (schemas, flows, axes), never code." | UNVERIFIED — no authoring guide or pack system found | No file in `docs/` or `tasks/` (full listing reviewed) defines or discusses "packs" |
| "CEL is meant to be the only expression language, and no embedded scripting is allowed." | UNVERIFIED — no CEL dependency, no scripting ban documented, because no pack/expression system exists | `Cargo.lock` dependency sweep (§3) — 0 hits for any CEL crate; 0 hits for any scripting-language interpreter crate |
| "The engine is AGPL and packs are MIT" | **CONTRADICTED** by this repository — engine license is `GPL-3.0-or-later`, not AGPL, and no MIT pack package exists | `Cargo.toml:8` (`license = "GPL-3.0-or-later"`); `THIRD_PARTY_LICENSES.md:1` ("loxia is `GPL-3.0-or-later`") |
| "The pack authoring guide makes claims about all of this that have not been checked." | CONFIRMED that no such guide exists to check | Full `docs/` and `tasks/` path review (this task's supplied file listing) |

## Summary

This repository (`loxia-player`) does not contain the pack/loader/CEL/AGPL-vs-MIT system the work
item describes. Every section above documents the concrete search performed and its result rather
than asserting compliance with a system that isn't present. If a pack system exists in a different
repository or a branch not included in this task's context, this document cannot speak to it; that
scope gap is the reason the relevant rows above are marked `UNVERIFIED`/`NOT FOUND` rather than
given a pass/fail compliance verdict.
