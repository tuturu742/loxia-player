# Reconcile loxia-audio documentation with its modules and backends

- title: Reconcile loxia-audio documentation with its modules and backends
- description: Context: crates/loxia-audio/src contains backend.rs, device/mod.rs, eq.rs, error.rs, gapless.rs, lib.rs, mock.rs, mpv.rs, mpv/filters.rs, mpv/handle.rs, mpv/props.rs, replaygain.rs, plus five insta snapshots under src/snapshots/ for eq tests (band_command_boost, band_command_cut, filter_string_boost, filter_string_cut, filter_string_flat). The project overview says audio 'operates independently behind a trait' and mentions gapless playback and ReplayGain. CONTRIBUTING.md's Definition of Done requires 'the crate's lib.rs module list updated'. None of these claims has been checked against the code.

Steps:
1. Read lib.rs. Compare its `mod`/`pub mod` declarations and any module list in its `//!` crate docs with the files on disk. Fix the `//!` list so it matches.
2. Read backend.rs, mpv.rs and mock.rs. Determine whether backend.rs declares a trait, which types implement it, and whether mock is compiled into normal builds or gated (`#[cfg(test)]`, a cargo feature). Check the device/ module's role too.
3. Find every doc claim about audio: `rg -n -i "gapless|replaygain|replay gain|backend|mpv|equali[sz]er|eq preset" docs/ *.md crates/loxia-audio/src`. Compare each claim with what gapless.rs, replaygain.rs, eq.rs and the backends actually do: modes supported, defaults, fallback behaviour, whether more than one real backend exists. Also compare with assets/eq_presets.toml for preset names.
4. Fix the docs (Markdown files and `//!`/`///` comments in loxia-audio) to describe the code. Do not change behaviour. If a doc promises a feature the code lacks, remove the claim, or mark it 'not implemented' if docs/12-decisions.md says it is planned.
5. Note in the PR description, without fixing it, that there is no band_command_flat snapshot to match filter_string_flat. Say whether eq.rs's tests explain why, or whether it looks like a test gap.

Files: crates/loxia-audio/src/*.rs (doc comments only), plus whichever docs/*.md files step 3 finds. Stay within this one crate's code. Do not edit other crates.

Done when: `cargo doc -p loxia-audio --no-deps` builds without warnings, the lib.rs module list matches the files exactly, and the PR table maps each audio doc claim (file:line) to the code fact (file:line) with the fix applied.

## Brief

<knowledge id="k1" class="lore" source="Why this project exists" entry="Business constraints">
Single maintainer: features that need staffing to operate are out of scope. Self-hosted first — no hosted service to sell, so the install must stay a one-liner. The engine is AGPL and the packs MIT, so anything that would force pack authors to open their content is a design error, not a licensing detail.
</knowledge>

<knowledge id="k2" class="lore" source="Why this project exists" entry="What users actually ask for">
In order of how often it comes up: 'the NPC blurted the twist' (the reason exclusion exists), 'the dice are made up', 'I can't tell why it said that', and 'I don't want my campaign on someone else's server'. Every one of those maps to a structural feature rather than a better prompt — that mapping is the product.
</knowledge>

<knowledge id="k3" class="lore" source="Why this project exists" entry="Who this is for">
Three audiences, in priority order. **Tabletop groups** who want a game master that can hold a secret and dice that cannot be talked out of a result. **Teams** running structured multi-agent working sessions where some facts are genuinely confidential. **Engineering orgs** delegating work to coding agents under review. The first pays the rent for the design; the other two prove the engine is domain-neutral.
</knowledge>

<knowledge id="k4" class="misc" source="Project reference shelf" entry="Decisions worth remembering">
Postgres-only was chosen over a vector database because the isolation guarantees live in RLS and a second store would need its own. CEL was chosen over any embedded scripting because user-authored code is a security stance we do not want to defend. Both decisions get re-proposed roughly twice a year; neither has changed.
</knowledge>

<knowledge id="k5" class="misc" source="Project reference shelf" entry="Team glossary">
**Overlay** — the per-workspace relabelling of core nouns. **Pack** — declarative workflow content (schemas, flo

## Status

Scaffolded by the delegated coding agent. TODO: implement.
