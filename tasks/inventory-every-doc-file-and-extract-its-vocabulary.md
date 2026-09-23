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

The work item was not done. The complete file list shows only two added files: src/inventory-every-doc-file-and-extract-its-vocabulary.js and tasks/inventory-every-doc-file-and-extract-its-vocabulary.md. The one required deliverable, docs/_reconciliation/findings-inventory.md, is missing. Both added files are visible in full, and both are scaffolding marked 'TODO: implement' / 'not yet implemented'. The truncated brief inside the task file does not limit this verdict, because the missing ledger is clear from the file list, which is never truncated.

Required changes:

1. Delete src/inventory-every-doc-file-and-extract-its-vocabulary.js. This item is a documentation ledger, not code. An empty exported stub with a TODO adds an unimplemented symbol to src/ and is out of scope.

2. Delete tasks/inventory-every-doc-file-and-extract-its-vocabulary.md. It restates the work item plus a pasted brief and is not a deliverable. The work item names exactly one file to create.

3. Create docs/_reconciliation/findings-inventory.md. Every entry must cite a real repo path and line number that you actually observed. Nothing should be taken from the brief or assumed. It needs these sections:

(a) Doc inventory. Run a full-tree search from the repo root, e.g. `git ls-files | grep -iE '\.(md|mdx|rst|txt|adoc)$|openapi|swagger'`. Paste the exact command and its complete, unedited output. Make sure the output covers READMEs at every depth, docs/ directories, the pack authoring guide, API/CLI reference, install docs, glossary, and any ground-rules or lore files. If any of these do not exist, say so explicitly. Do not omit them silently.

(b) Provenance. For each listed file, mark it hand-written or generated. For generated files, quote the exact line in package.json scripts, the Makefile, or CI config that produces it, with its path:line.

(c) Architectural claims in code comments. First check the listing for the real source directory, since packages/ may not exist. Run the grep against th

## Status

Scaffolded by the delegated coding agent. TODO: implement.
