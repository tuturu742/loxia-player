# Decide whether loxia-tui snapshots or rendering are wrong, then make the five failing snapshot tests pass

- title: Decide whether loxia-tui snapshots or rendering are wrong, then make the five failing snapshot tests pass
- description: Problem: on a clean `master`, `cargo test --workspace` fails five insta snapshot tests. They are all in loxia-tui:
- `render::tests::layout_snapshot_80x24`
- `render::tests::layout_snapshot_120x30`
- `render::tests::layout_snapshot_200x50`
- `views::now_playing::tests::now_playing_snapshot_history`
- `widgets::header::tests::header_snapshot_offline_with_downloads`

This is one small task for one person. Don't refactor anything, and don't touch tests that already pass.

Scope: `crates/loxia-tui` only. That means its `src/render.rs`, `src/views/now_playing.rs`, `src/widgets/header.rs` and their committed `snapshots/*.snap` files. Confirm the exact paths with `git ls-files crates/loxia-tui | grep -E 'snap|render|now_playing|header'`. If the root cause turns out to be in another crate (for example a loxia-core model or keymap change), stop. Write up the cause in the PR and hand it back instead of editing that crate. CONTRIBUTING.md forbids crossing crate boundaries without an authorising task.

Step 1: decide before you change anything. Write the verdict at the top of the PR description.
1. Run `cargo test -p loxia-tui` and capture the insta diff for each of the five tests (`cargo insta test -p loxia-tui --review`, or read the generated `.snap.new` files). Don't accept anything yet.
2. For each stored `.snap` file, find the commit that last changed it (`git log --follow -- <snap>`). Then find the commits since then that touched the rendering code behind it (`git log <that-commit>..master -- crates/loxia-tui crates/loxia-core/src/keymap*`). Identify which commit changed the rendering.
3. Classify each diff as one of three kinds:
   (a) Snapshots stale. The rendering change was intentional: a commit or task deliberately changed the layout, header or history view, and the new output matches the design docs under `docs/` and the task file under `tasks/` that introduced it.
   (b) Rendering regressed. The change was unintended, e.g. a truncated or misaligned column, a lost offline or download indicator, history order or content wrong, or text that contradicts `docs/`. Also check the hard rule that UI text renders keybindings through `KeyMap::hint_for(ActionId)` and never hardcodes them.
   (c) Nondeterministic output: wall-clock time, relative timestamps, locale, hostname, environment or terminal size leaking into the render. If the test fails differently between two runs, or with `TZ`/`LANG` changed, it is (c).
4. Write down the verdict for each test: (a), (b) or (c), with the commit hash and one sentence of evidence. The five tests may not all get the same verdict.

Step 2: act on the verdict.
- (a): accept only these five snapshots (`cargo insta accept`, or rename the matching `.snap.new` files). Check that the diff of each accepted `.snap` matches exactly the intentional change you cited.
- (b): fix the rendering code in loxia-tui so that output matches the stored snapshot again. Don't edit the `.snap` files. Add nothing beyond the fix.
- (c): make the test deterministic by injecting a fixed clock or fixture value in the test setup, then accept the resulting snapshot. Don't use insta redactions to hide real content.
- Forbidden: deleting snapshots, setting `INSTA_FORCE_PASS` or `INSTA_UPDATE` in CI or config, adding `#[ignore]`, weakening assertions, and adding dependencies not listed in `docs/13-dependencies.md`. The code must not use `unwrap()`/`expect()` outside tests.

Files changed: only the `.snap` files for the five named tests, and/or the loxia-tui source behind them. `git diff --stat` must show nothing else. No stray `.snap.new` or `.pending-snap` files may be committed.

Done when:
- All five named tests pass.
- `cargo test --workspace`, `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean on the branch (`just check-all`).
- The PR description states the verdict for each test (stale, regression or nondeterministic), with the commit hash and evidence, and says whether snapshots or code were changed.
- A reviewer can check the verdict by reading the cited commit next to the snapshot diff.

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
