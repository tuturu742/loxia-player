# Correct the crate/package map so it lists exactly the workspace members that exist

- title: Correct the crate/package map so it lists exactly the workspace members that exist
- description: CONTEXT: Pyrrhula's docs include a "crate map" (also called a package map or layout section) that lists the workspace's crates/packages and what each one does. It has drifted from the code. Nobody has checked the repo for this task, so do not assume any path exists until you have listed it yourself.

STEP 1 - Find the map. Run `git grep -nIiE 'crate map|package map|packages/|crates/|workspace members' -- '*.md' '*.mdx' '*.rst'` and paste the command and its output into the PR description. Pick the file(s) and line range(s) that list crates/packages. If several docs have their own lists, fix all of them. If no such list exists anywhere, stop and report that in the PR/task comment. Do not write a new map from scratch.

STEP 2 - Get the ground truth. Paste each command and its full output into the PR description:
- the workspace config: `cat Cargo.toml` ([workspace] members), `cat package.json` ("workspaces") and `cat pnpm-workspace.yaml`, whichever of these exist. Say explicitly which ones do not exist.
- the actual directories: `git ls-files | grep -E '^(packages|crates)/[^/]+/(Cargo.toml|package.json)$'`
- for each member, the name and description fields from its own manifest.

STEP 3 - Edit ONLY the map section(s) you found in step 1:
- every workspace member appears exactly once, under its real path and name
- remove entries that point to paths that do not exist
- write each member's one-line description from its manifest description or its README first line, not from guesswork. If neither exists, write "(no description in manifest)".
- do not change any other part of the doc. Do not touch code, manifests or the docs/_reconciliation/ ledger files.

STOP-AND-ESCALATE RULE: Under the project invariant, anything under packages/core/ must be named in domain-neutral terms. Domain words such as campaign, NPC, quest, player, dice, session, ticket or PR belong only in workflow packs and vocabulary overlays. If a member under packages/core/ has a domain word in its name or description, still list it accurately. Also add a line "ESCALATE: <path> uses domain term <word>" to the PR description, and do not describe the name as intended.

DONE WHEN (the reviewer checks the diff, not this summary): (a) the diff touches only the map section(s) found in step 1; (b) the set of paths in the edited map equals the output of the step-2 ls-files command, and the reviewer can rerun that command and compare line by line; (c) every description can be traced to a manifest or README line cited in the PR; (d) any ESCALATE lines are present or explicitly marked 'none'.

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
