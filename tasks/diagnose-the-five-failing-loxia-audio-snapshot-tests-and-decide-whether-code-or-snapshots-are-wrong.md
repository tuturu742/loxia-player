# Diagnose the five failing loxia-audio snapshot tests and decide whether code or snapshots are wrong

- title: Diagnose the five failing loxia-audio snapshot tests and decide whether code or snapshots are wrong
- description: CONTEXT: `cargo test` fails on trunk. Five insta snapshot tests in crates/loxia-audio fail: band_command_boost, band_command_cut, filter_string_boost, filter_string_cut, filter_string_flat. CI builds only when the tests pass, so trunk is blocked. Nobody has looked at the repo for this task yet. List files yourself before you assume any path exists.

This item is diagnosis only. Do NOT change code, tests or snapshot files. Do NOT run `cargo insta accept` or `cargo insta review`, and do not set INSTA_UPDATE or INSTA_FORCE_PASS. Put the output in the task comment, not in the repo.

STEPS (paste each command and its full output in the task comment):
1. `cargo test -p loxia-audio 2>&1 | tee /tmp/loxia-test.log; echo exit=$?`. Paste the insta diff for each of the five tests: the old snapshot next to the new value.
2. Find the tests and their snapshots: `git grep -nE 'fn (band_command_boost|band_command_cut|filter_string_boost|filter_string_cut|filter_string_flat)' -- crates/loxia-audio` and `git ls-files crates/loxia-audio | grep -E '\.snap$'`. For each test, give the test file:line, the snapshot file path and the function(s) under test (file:symbol).
3. Read the code that produces each snapshotted value, meaning the band-command and filter-string builders and everything they call. Summarise what the code does now for boost, cut and flat: gain sign, units, frequency/Q formatting, rounding, field order and separators.
4. History: run `git log --follow -p -- <each snapshot file>` and `git log -p -L :<symbol>:<file>` (or `git log -p -- <file>`) for the code under test. Find the commit that made the snapshots and code disagree. Quote its hash, message and the relevant hunk. Say whether the change looks intentional (message or linked issue describes a behaviour change) or accidental (refactor, dependency bump, formatting change, or sign/units slip).
5. Check any external contract: whatever consumes the band command or filter string (e.g. an external tool's filter syntax, a device protocol, docs or comments in the crate). Quote the spec or comment that says what the correct output is. Check whether a dependency bump (`git log -p -- Cargo.lock` near the breaking commit) changed float formatting or similar.
6. VERDICT, one row per test: test | snapshot says | code produces | correct per (spec/comment/commit citation) | wrong side: CODE or SNAPSHOT. If the evidence cannot settle a test, say UNDECIDED and explain what is missing. Do not guess.

DONE WHEN: the task comment has all five rows with a CODE/SNAPSHOT/UNDECIDED verdict, each backed by a quoted citation (spec, comment, or commit hash + hunk), and a reviewer can rerun every pasted command and get the same result. The follow-up fix item works only from this verdict table.

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
