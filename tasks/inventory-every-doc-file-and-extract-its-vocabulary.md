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

Scope is correct. The file list shows exactly one added file, docs/_reconciliation/findings-inventory.md, and no existing doc was edited.

The §0 finding is valuable and should stay. The brief assumed packages/, a vocabulary-neutral core and tabletop lore, but the repo is the `loxia` Rust workspace, and the ledger records that mismatch instead of forcing the wrong glossary onto it. I'm also glad nothing was fabricated.

However, the DONE WHEN criteria are not met. The ledger says that no command was executed and most doc bodies were never read. All requested fixes are in docs/_reconciliation/findings-inventory.md:

1. §1: Replace the "transcription" with real output. Run `git ls-files | grep -iE '\.(md|mdx|rst|txt|adoc)$|openapi|swagger'` in the checkout and paste stdout verbatim, followed by the output of `| wc -l`. Resolve the `design_overview` and `svg` follow-ups now instead of deferring them:
   - Run `file design_overview svg` and `head -20` on each, and paste the results.
   - Classify each file.
   - If `design_overview` is a prose doc, add it to the inventory and state that the regex misses extensionless docs.
   Also resolve the missing root README by pasting the output of `git ls-files | grep -i readme`.

2. §2: Read every file listed in §1 and replace each "Presumed hand-written; NOT INSPECTED" row with a determination backed by evidence. Either quote a generator banner with its line number, or state that `grep -nEi 'generated|do not edit|auto-?gen' <file>` returned nothing and paste that command. Paste the output of `cat justfile` (or at least `grep -n '' justfile`), then quote any recipe that writes docs, or state that none does.

3. §3: Run `grep -rnE '(TODO|NOTE|INVARIANT|guarantee|always|never)' crates/` across all crates, not only loxia-audio. Paste the raw output, then add a case-insensitive `-i` rerun, since you already noticed the capitalised "Never" miss. Then list the architectural claims from that output. loxia-core is the priority because of

## Status

Scaffolded by the delegated coding agent. TODO: implement.
