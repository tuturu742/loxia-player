# Fix failing snapshot test views::now_playing::tests::now_playing_snapshot_history (crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap)

- title: Fix failing snapshot test views::now_playing::tests::now_playing_snapshot_history (crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap)
- description: Fixture: `crates/loxia-tui/src/views/snapshots/loxia_tui__views__now_playing__tests__now_playing_snapshot_history.snap`
Test: `views::now_playing::tests::now_playing_snapshot_history` in `crates/loxia-tui/src/views/now_playing.rs`
Rendering code: `crates/loxia-tui/src/views/now_playing.rs`, the now-playing view with its play-history list.

Background: on a clean `master`, `cargo test --workspace` fails five insta snapshot tests in loxia-tui. Each one is its own task, and this task covers only the fixture above. The sibling fixtures are the three render layout snapshots and header_snapshot_offline_with_downloads. Do not edit their `.snap` files.

Step 1: decide before you change anything, and put the verdict at the top of the PR description.
1. Read the current stored `.snap` above in full, and read `now_playing.rs` (the test fixture setup and the history rendering code).
2. Run `cargo test -p loxia-tui views::now_playing::tests::now_playing_snapshot_history`. Capture the insta diff from the `.snap.new` or from `cargo insta test -p loxia-tui --review`. Do not accept it yet.
3. Run `git log --follow -- <fixture>` to find the commit that last changed the fixture. Then run `git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*` to find the commit that changed the rendering.
4. Classify the diff as one of:
   (a) Stale fixture: a commit or task under `tasks/` changed the history view deliberately, and the new output matches the design docs under `docs/`.
   (b) Rendering regression: the change was not intended. Examples are history in the wrong order (check the order `docs/` specifies), missing or duplicated entries, wrong fields or truncation, or a keybinding hardcoded instead of rendered through `KeyMap::hint_for(ActionId)`.
   (c) Nondeterministic output: history views often render relative times such as '3 min ago' or wall-clock times. Check whether the render reads the system clock, locale or timezone. Run the test twice, and once with `TZ=UTC LANG=C` changed. If the output differs, the verdict is (c).
5. Write the verdict with the commit hash and one sentence of evidence.

Step 2: act on the verdict.
- (a): rewrite only this `.snap` file with the new output (`cargo insta accept` for this snapshot, or rename its `.snap.new`). Keep insta's header block (`---`, `source:`, `expression:`, `---`) in the same format. Check that the diff matches exactly the intentional change you cited.
- (b): fix `now_playing.rs` so the output matches the stored fixture again. Do not edit the `.snap`, and add nothing beyond the fix.
- (c): make the test deterministic. Inject a fixed clock or fixed 'now' value into the test setup, and if the view has no seam for that, add a minimal one inside loxia-tui. Then accept the resulting snapshot. Do not use insta redactions to hide real content.
- If the root cause is in another crate (for example a loxia-core model or history-state change), stop. Write up the cause in the PR and hand the task back. CONTRIBUTING.md forbids crossing crate boundaries without an authorising task.

Forbidden: deleting snapshots, setting `INSTA_FORCE_PASS` or `INSTA_UPDATE`, adding `#[ignore]`, weakening assertions, adding dependencies not listed in `docs/13-dependencies.md`, and `unwrap()`/`expect()` outside tests. Do not refactor, and do not touch tests that already pass.

Done when:
- The test passes, and it passes the same way under two runs and under a changed `TZ`/`LANG`.
- `git diff --stat` shows only this fixture and/or `now_playing.rs` (or other loxia-tui source it calls).
- No stray `.snap.new` or `.pending-snap` files are committed.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
- The PR states the verdict ((a), (b) or (c)), the commit hash and the evidence, and says whether the snapshot or the code changed.

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
