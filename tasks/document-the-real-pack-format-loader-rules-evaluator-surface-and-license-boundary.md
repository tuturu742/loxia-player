# Document the real pack format, loader rules, evaluator surface and license boundary

- title: Document the real pack format, loader rules, evaluator surface and license boundary
- description: WHY: Packs are meant to be declarative workflow content (schemas, flows, axes), never code. CEL is meant to be the only expression language, and no embedded scripting is allowed. The engine is AGPL and packs are MIT, so nothing in the design may force pack authors to open their content. The pack authoring guide makes claims about all of this that have not been checked.

CREATE: docs/_reconciliation/findings-packs.md. Do NOT edit existing docs or code.

CONTENTS:
1. Pack format: the loader entry point and its validation schema (file:symbol). List every top-level and nested field the loader accepts, with its type and whether it is required. Mark each field against the authoring guide as documented / undocumented / documented-but-absent.
2. Accept/reject rules: quote the validation errors the loader can raise. Name any tests that exercise rejection.
3. Evaluator surface: exhaustively grep all packages for alternatives to CEL: eval, new Function, vm/vm2, embedded Lua/JS/Python interpreters, template engines with logic, dynamic import of pack-supplied paths. Paste the patterns and all hits. Any path that runs pack-authored code is marked ESCALATE.
4. License boundary: state where pack content lives and how it is loaded relative to the engine tree (separate package? runtime data? bundled?). Quote the LICENSE files and package license fields you find. Mark any coupling that would make pack content a derivative of the AGPL engine as ESCALATE.

DONE WHEN: The field table is complete, the evaluator search shows its patterns and zero-hit results, and every authoring-guide claim has a verdict with a citation.

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
