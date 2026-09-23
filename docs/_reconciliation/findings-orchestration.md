# Findings: context assembly, disclosure gate, turn manifest, idempotency and resume

Status: **ESCALATE (scope mismatch).** This file must be read before any of the per-section findings.

## 0. Summary

The task asks for verification of four claims:

- concealed plaintext is excluded from a model's generation context;
- a gist-only disclosure gate fails closed to conceal;
- a per-turn context manifest exists;
- idempotency-keyed side effects make resume safe for a tenant's paid API key.

The repository in front of me is **loxia**, which describes itself as "a terminal music client for Emby".

- `Cargo.toml` `[workspace.package]`: `repository = "https://github.com/tuturu742/loxia-player"`
- `assets/BRANDING.md`, banner block: `a terminal music client for Emby`
- `crates/loxia-audio/src/lib.rs` (module doc): `Playback engine: the AudioBackend abstraction, the libmpv2-backed implementation, and a deterministic mock.`

The workspace has six crates: `loxia-core`, `loxia-emby`, `loxia-audio`, `loxia-cache`, `loxia-tui` and `loxia-player`. Their direct dependency list in `Cargo.toml` `[workspace.dependencies]` contains:

- no LLM or model-client crate;
- no database driver;
- no billing SDK.

Networking is limited to:

- `reqwest` for the Emby HTTP API;
- `tokio-tungstenite` for the Emby WebSocket.

The session background describes a different project. It mentions workspaces, packs, a Steward, Postgres/RLS, CEL, `packages/core/` and `tenant_id`. None of those appear anywhere in this repository's file tree. Concretely:

- there is no `packages/` directory;
- no `.sql` files exist;
- no file path contains `gate`, `context`, `prompt`, `conceal`, `gist`, `tenant` or `idempot`.

**Conclusion:** the claims being audited do not correspond to any code in this repository. They are neither implemented nor contradicted. Each section below records that finding with the evidence available.

The finding needs a human decision. One of two things is true:

- the task was dispatched against the wrong repository; or
- the docs being reconciled describe a system that does not exist here.

The prerequisite input `docs/_reconciliation/findings-inventory.md` does **not exist** in this repository. It is absent from the file list, and `docs/_reconciliation/` contains no other file. Section 5 therefore cannot be completed as specified.

### Limits of this audit (read before trusting any negative claim)

I did not have a shell. My evidence is limited to two things:

1. **The complete repository file list** (paths). This is authoritative for path-based searches.
2. **File contents provided to me.** These are complete for some files and truncated or empty for most others. Every file under `crates/loxia-core`, `crates/loxia-emby`, `crates/loxia-cache`, `crates/loxia-player`, `crates/loxia-tui` and `docs/` was shown with **no content**. The same applies to `crates/loxia-audio/src/mpv/handle.rs`.

Negative claims below are therefore of two strengths:

- **Path-level (verified):** based on the full file list.
- **Content-level (NOT verified):** these require running the commands given in each section. Each command is listed so a reviewer can reproduce or refute the finding. I do not report their output because I did not run them.

---

## 1. Context assembly

**Finding:** no function in this repository builds a model/LLM generation context. There is no concealed-item filter to quote. **ESCALATE (claim has no implementation to verify).**

Path-level evidence (verified against the file list):

- No path contains `context`, `prompt`, `llm`, `model_client`, `openai`, `anthropic`, `completion`, `conceal`, `secret` or `gist`.
- The `model/` directory is `crates/loxia-core/src/model/`. Its files are `audio_meta.rs`, `ids.rs`, `image.rs`, `item.rs`, `lyrics.rs` and `playback.rs`. These are music domain types, not a language model.
- The only quoted contents available confirm this. `crates/loxia-audio/src/backend.rs` imports `use loxia_core::model::{AudioDevice, AudioFormat};`.

Content-level searches (must be run; results NOT verified):

```sh
git grep -nIiE 'prompt|system_message|messages\.push|chat|completion|llm|openai|anthropic|embedding' -- crates scripts
git grep -nIiE 'conceal|conceled|concealed|plaintext|gist|redact' -- crates
grep -nE '^(async-openai|openai|anthropic|llm|genai|ollama)' Cargo.toml crates/*/Cargo.toml
```

The nearest analogue to "excluded, not instructed away" is secret redaction. `crates/loxia-core/src/effect.rs:RedactedUrl` keeps stream-URL `api_key` values out of `Debug` output. Its only visible use is `crates/loxia-audio/src/backend.rs:AudioCommand::Load`:

```rust
        // A plain `String` per this task's own spec would defeat the point of redacting it —
        // `loxia_core::effect::RedactedUrl` already exists for exactly this (a stream URL's
        // `api_key=...` query parameter must never reach a derived `Debug`), ...
        url: RedactedUrl,
```

That test is `backend.rs::tests::load_command_debug_redacts_api_key`. Its body was truncated in the context I received.

This is log hygiene, not generation-context exclusion. It must not be cited as evidence for claim (a).

**Plaintext-plus-instruction path:** none can exist, because there is no prompt construction at path level. This rests on the path-level evidence above plus the content-level grep, which has not been run.

## 2. Disclosure gate

**Finding:** no gate entry point, no gate input type, and no conceal/hint/reveal decision exist. There are no error, timeout or default branches to quote. No test covers fail-closed behaviour. **ESCALATE.**

Path-level evidence (verified): no path contains `gate`, `disclosure`, `reveal` or `hint`.

The "hint" occurrences the task might collide with are unrelated:

- `KeyMap::hint_for(ActionId)`, per `CONTRIBUTING.md`: "render through `KeyMap::hint_for(ActionId)`". This is a keybinding hint.
- `crates/loxia-audio/src/error.rs:library_not_found_hint`. This is an install hint.

Content-level searches (NOT verified):

```sh
git grep -nIiE '\b(gate|disclos|reveal|conceal)\w*' -- crates
git grep -nIiE 'enum\s+\w*(Decision|Disclosure)' -- crates
git grep -nIiE 'fail[_ -]?closed|default.*conceal' -- crates docs
```

## 3. Manifest (per-turn context record)

**Finding:** there is no per-turn context manifest. **ESCALATE.**

A file named `crates/loxia-cache/src/manifest.rs` exists. The task file `tasks/phase-08-cache-offline/08-02-manifest-and-lru.md` places it with the LRU cache. It is a **media-cache manifest**, not a record of what a model turn's context contained.

Its contents were not provided to me, so I cannot quote its type or fields. The name collision is itself worth recording, so that a later reconciler does not mistake this file for the claimed turn manifest.

Search to confirm (NOT verified):

```sh
git grep -nIE 'struct\s+\w*Manifest|fn\s+\w*manifest' -- crates
```

## 4. Idempotency and resume

**Finding:** the concepts of a paid model call, a tenant API key and billing have no counterpart here.

- **Paid model calls:** none (path-level; no model-client dependency in `Cargo.toml`).
- **Billing:** none (path-level; no billing module or dependency).
- **Tenant API key:** none. The only credential is the user's own Emby access token. Examples: the CI fixture scan in `.github/workflows/ci.yml` checks for `"AccessToken"`, and the `api_key` query parameter is redacted by `RedactedUrl`.

The repository does have side-effecting operations with external effect: outbound HTTP to a self-hosted Emby server, and local filesystem writes. None of them can "double-charge" anyone.

Candidate list, path-level only. Contents were not provided, so the idempotency status of each is **UNVERIFIED**, not MISSING:

| Candidate side effect | Location (path) | Key accepted/checked? |
| :-- | :-- | :-- |
| Playback start/progress/stopped reporting (HTTP POST) | `crates/loxia-emby/src/endpoints/playback.rs` | UNVERIFIED |
| Mark played / scrobble | `endpoints/playback.rs`, `crates/loxia-cache/src/scrobble.rs` | UNVERIFIED |
| Favourite toggle | `crates/loxia-emby/src/endpoints/favorites.rs` | UNVERIFIED |
| Playlist create/append | `crates/loxia-emby/src/endpoints/playlists.rs` | UNVERIFIED |
| Download to disk | `endpoints/download.rs`, `crates/loxia-cache/src/downloads.rs`, `crates/loxia-player/src/workers/download_fetcher.rs` | UNVERIFIED |
| HTTP retry policy (re-issue risk) | `crates/loxia-emby/src/retry.rs` | UNVERIFIED |
| Session persistence / restore | `crates/loxia-cache/src/session.rs`, `tasks/phase-11-settings/11-06-session-restore-wiring.md` | UNVERIFIED |
| WebSocket remote control | `crates/loxia-emby/src/ws.rs` | UNVERIFIED |
| Desktop notifications, MPRIS | `crates/loxia-player/src/workers/notify.rs`, `workers/mpris.rs` | UNVERIFIED (local only) |

Grep to complete this table exhaustively (NOT run):

```sh
git grep -nIE '\.(post|put|delete|patch)\(|Method::(POST|PUT|DELETE)|reqwest::Client' -- crates
git grep -nIE 'fs::write|File::create|OpenOptions|rename\(' -- crates
git grep -nIiE 'idempot|dedup|request_id|x-request-id' -- crates
```

Two follow-ups are needed:

- **Scrobble replay.** A scrobble buffer that replays after going offline (`08-07-scrobble-buffer.md`) is the one place where a duplicate external effect is plausible: a duplicate "played" report. Whether replay is deduplicated must be checked in `loxia-cache/src/scrobble.rs` once its contents are available. This is a correctness question for play counts, not a billing one.
- **Resume.** The resume path is session restore: queue, position and history (`08-08`, `11-06`). By construction it cannot re-issue a paid model call, because no such call exists anywhere in the dependency graph. Whether restore re-sends a playback-start report is UNVERIFIED.

## 5. Comparison with docs

This section **cannot be completed.**

- `docs/_reconciliation/findings-inventory.md` does not exist in this repository (verified against the file list). There is no inventory of doc claims to mark match / drift / ESCALATE.
- The contents of `docs/01`–`14` and `docs/README.md` were not provided to me. I cannot quote any doc claim about concealment, gates, turn manifests or idempotency.
- I have no evidence that such claims exist in those docs. Their titles cover architecture, data model, the Emby API, state and input, the audio engine, cache and offline, UI, the roadmap, traceability, testing, packaging, decisions, dependencies and the manual test plan. That is consistent with a music client.

Search a reviewer should run (NOT run):

```sh
git grep -nIiE 'conceal|gate|manifest|idempot|tenant|plaintext|resume' -- docs design_overview
```

The expected outcome is hits for "manifest" (cache) and possibly "resume" (session/playback), with no hits for the orchestration sense of these terms.

## Escalations

1. **Wrong-repository or phantom-claim question.** Claims (a)–(d) have no implementation in loxia. A human must decide whether this reconciliation belongs to another repository, or whether some doc here wrongly makes these claims.
2. **Missing input.** `docs/_reconciliation/findings-inventory.md` is absent. Section 5 is blocked on it.
3. **Unverified content-level negatives.** All grep commands above must be run before any negative claim here is treated as final. My audit had file paths but not file contents for most crates.
