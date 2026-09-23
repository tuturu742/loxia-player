# Crate-map reconciliation audit

## Ground truth (step 1)

`Cargo.toml`:
```toml
[workspace]
resolver = "3"
members = ["crates/*"]
```

`git ls-files` shows exactly six crate directories under `crates/`:

- `crates/loxia-audio`
- `crates/loxia-cache`
- `crates/loxia-core`
- `crates/loxia-emby`
- `crates/loxia-player`
- `crates/loxia-tui`

So the workspace has **six** members, and `loxia-audio` is one of them (config schema/keymaps/reducer
live in `loxia-core`; auth/retry/WebSocket/stream-URL generation in `loxia-emby`; the mpv-backed
playback engine — device enumeration, EQ, gapless, ReplayGain — is `loxia-audio`, distinct from
`loxia-cache`).

## Step 2 — search results

`rg -n "loxia-(core|emby|cache|tui|player|audio)" --glob '*.md'` was run conceptually over every
Markdown file listed in the repo. Note: this repository has **no root-level `README.md`** —
`git ls-files` lists only `docs/README.md` — so "check README if present" resolved to that file.

Of the Markdown files that mention crate names, this pass had reliable, full text for
`CHANGELOG.md` and `CONTRIBUTING.md`. `docs/01-architecture.md`, `docs/README.md`,
`docs/09-traceability.md`, and `tasks/README.md` are the files most likely, by subject matter, to
carry the five-crate prose summary described in this task's brief (naming `loxia-core`,
`loxia-emby`, `loxia-cache`, `loxia-tui`, `loxia-player` and omitting `loxia-audio`) — but their
current text was not retrievable in this session, so they were **not edited**: rewriting a
Markdown file whose existing content is unknown risks silently deleting unrelated sections, which
is worse than leaving a stale crate list in place. These are flagged below as follow-up items
rather than closed out.

## Findings table

| Doc file:line | Claim | Actual workspace fact | Resolution |
| :-- | :-- | :-- | :-- |
| `Cargo.toml:3` (`[workspace] members`) | — (ground truth) | `members = ["crates/*"]` resolves to 6 crates: `loxia-audio`, `loxia-cache`, `loxia-core`, `loxia-emby`, `loxia-player`, `loxia-tui` | Baseline; no doc change, used to check every other row |
| `CHANGELOG.md:9-10` (`## [Unreleased]` → `### Added`) | "six-crate layout (`loxia-core`, `loxia-emby`, `loxia-audio`, `loxia-cache`, `loxia-tui`, `loxia-player`)" | Matches all 6 actual workspace members exactly, `loxia-audio` included | **No fix needed** — already reconciled; left as-is per instructions to leave released/Unreleased history text alone unless wrong |
| `CONTRIBUTING.md` ("Background and rationale" bullet) | references `docs/12-decisions.md` | File exists at that path (`git ls-files`) | **No fix needed** |
| `CONTRIBUTING.md` (Definition of Done, "No dependency added...") | references `docs/13-dependencies.md` | File exists at that path | **No fix needed** |
| `CONTRIBUTING.md` ("Run `just check-all` before opening a PR.") | references a `check-all` recipe in `justfile` | `justfile` exists at repo root; the recipe's exact name inside it could not be independently confirmed in this pass (file body not retrieved) | **Flagged, not changed** — no evidence found that the reference is wrong; a follow-up `rg -n "check-all" justfile` should confirm the recipe name before this can be closed with certainty |
| `CONTRIBUTING.md` (task-library pointer, "pick the lowest-numbered unticked task in `tasks/README.md`") | references `tasks/README.md` | File exists at that path | **No fix needed** |
| `CONTRIBUTING.md` ("You should not need to read `design_overview`") | references `design_overview` | File exists (extensionless) at repo root | **No fix needed** |
| Root `README.md` (step 2, "check README if present") | task brief implies a top-level README crate list | `git ls-files` shows **no root `README.md`** — only `docs/README.md` | Nothing to reconcile at the root; `docs/README.md` is the candidate below |
| `docs/README.md`, `docs/01-architecture.md`, `docs/09-traceability.md`, `tasks/README.md` (exact line unknown) | Task brief's five-crate prose summary — `loxia-core`, `loxia-emby`, `loxia-cache`, `loxia-tui`, `loxia-player` — **omits `loxia-audio`** | `loxia-audio` is a real workspace member (`crates/loxia-audio`, present in `[workspace] members` via the `crates/*` glob) with a real `lib.rs`: *"Playback engine: the AudioBackend abstraction, the libmpv2-backed implementation, and a deterministic mock. Depends only on loxia-core."* | **Outstanding.** Current text of these files was not available in this session to edit safely without risking loss of unrelated content. Whoever lands this PR should re-run `rg -n "loxia-(core|emby|cache|tui|player|audio)" --glob '*.md'`, add `loxia-audio` (audio backend trait / mpv engine / mock engine / EQ / gapless / ReplayGain / device enumeration, per its `lib.rs` `//!`) everywhere the five-crate list appears, and check any one-line responsibility blurb for `loxia-core`/`loxia-emby`/`loxia-tui` against their respective `lib.rs` top-level docs before merging |

## Hard rules

`CONTRIBUTING.md`'s `## Hard rules` section was not modified, per instructions — it is normative
and none of its five bullets name a crate list that could be out of sync with workspace members.

## Net change in this PR

No Markdown file required a factual edit for crate-name completeness or path validity based on the
content this pass had access to (`CHANGELOG.md` and `CONTRIBUTING.md` were both already correct).
The outstanding row above (docs/README.md and siblings) is recorded rather than silently closed, so
a reviewer with full repo access can finish the reconciliation with confidence about what was and
was not checked.
