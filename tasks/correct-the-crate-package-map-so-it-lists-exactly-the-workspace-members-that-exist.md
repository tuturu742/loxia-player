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

The file list is complete and shows 0 changed files, so this branch changes nothing. It is not a truncation problem.

The work item asks for an edit to the crate/package map section(s). An empty diff is only acceptable in two cases, and neither can be confirmed from what I can see:
- (1) STEP 1's `git grep` found no map anywhere, in which case the task says stop and report.
- (2) The existing map already matches the step-2 ground truth exactly.

DONE criteria (a) to (d) cannot be checked against an empty diff.

The fix depends on which situation applies:

1. **A map exists and is out of date.** Push a commit that edits only the map section(s) in the doc file(s) found by the STEP 1 grep.
   - Every path from `git ls-files | grep -E '^(packages|crates)/[^/]+/(Cargo.toml|package.json)$'` appears exactly once, under its real path and name.
   - Entries for paths that do not exist are removed.
   - Each description comes from the member's manifest description or README first line, or reads "(no description in manifest)" if neither exists.
   - Do not touch code, manifests, docs/_reconciliation/, or any other part of the doc.

2. **No map exists, or the map is already exactly correct.** Leave the branch empty, but the PR description must include:
   - the full STEP 1 grep command and its output
   - the STEP 2 workspace-config commands and outputs, naming which of Cargo.toml, package.json and pnpm-workspace.yaml are absent
   - the ls-files output
   - an explicit statement of which case applies and why

   Only then can this be accepted as a no-op.

3. **Either way**, include the ESCALATE lines for any packages/core/ member whose name or description contains a domain term (campaign, NPC, quest, player, dice, session, ticket, PR), or state 'ESCALATE: none'.

This is round 2 of 2. If the branch stays empty and there is no documented reason for it, I will escalate to a human rather than approve.

## Status

Scaffolded by the delegated coding agent. TODO: implement.
