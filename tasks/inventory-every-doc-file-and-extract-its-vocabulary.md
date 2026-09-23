# Inventory every doc file and extract its vocabulary

- title: Inventory every doc file and extract its vocabulary
- description: WHY: The docs have drifted from the code, and the job is to make the docs match what the code actually contains. No one has yet listed which docs exist. Nothing below has been verified: no repo contents have been seen by the planners, so do not assume any path exists until you have listed it.

CREATE: docs/_reconciliation/findings-inventory.md. This is a working ledger on the reconciliation branch. Do NOT edit any existing doc in this item.

CONTENTS:
1. A full list of documentation files. Produce it by running a full-tree search (e.g. `git ls-files | grep -iE '\.(md|mdx|rst|txt|adoc)$|openapi|swagger'`) and paste both the exact command and its full output. Include READMEs at any depth, docs/ directories, the pack authoring guide, API/CLI reference, install docs, glossary, and any in-repo ground-rules or lore files.
2. For each file: whether it is hand-written or generated. If generated, name the script, build target or source it comes from (quote the line in package scripts, Makefile or CI config).
3. Claims made in code comments: run `grep -rnE '(TODO|NOTE|INVARIANT|guarantee|always|never)' packages/` (adjust the path to whatever the listing shows) and list any comment that makes an architectural claim.
4. A domain-vocabulary list: every domain-specific noun the docs use for things core is supposed to name neutrally (e.g. campaign, NPC, quest, player, dice, session, ticket, PR). Record the file and line where each is used. A later work item greps core for these, so the list must be complete.
5. The documented install command, quoted verbatim with its source file. Run it in a clean container with no prior checkout (state the image used) and paste the full output and exit code.

DONE WHEN: The ledger exists. Every entry cites a real path with a line number. Commands and raw output are included so a reviewer can rerun them and get the same list. The install result is pass or fail with the log attached.

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
