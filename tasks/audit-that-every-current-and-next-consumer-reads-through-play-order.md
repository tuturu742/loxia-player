# Audit that every current and next consumer reads through play_order

- title: Audit that every current and next consumer reads through play_order
- description: Why: the play sequence is `entries[play_order[i]]`. Any consumer that indexes `entries[position]` directly shows the wrong track when shuffle is on. Three consumers are untraced:
1. The code that builds `PlaybackReport` for `Effect::Net::ReportPlayback`. The type lives in `crates/loxia-core/src/model/playback.rs`, and `loxia-emby::endpoints::playback::report` just posts what it is given. Construction is probably in `crates/loxia-core/src/reducer/`, but nobody has confirmed that.
2. The loxia-tui queue view and `widgets::player_bar`.
3. MPRIS in loxia-player. The repo overview lists an MPRIS background worker, but nobody has located the code. Check what it reports as the current track, the track list, and `CanGoNext`/`CanGoPrevious`.

Work: grep the workspace for `play_order`, `entries[`, `.entries.get(`, `PlaybackReport::` and `mpris`. For each read site, record the file, the symbol, and whether it goes through `play_order`. Also cover any queue-state helper such as a `current()` or `peek_next()` method.

This task changes nothing outside loxia-core, and makes no fixes. If a site outside core is wrong, register a separate single-crate task for it in `tasks/README.md`. If a core helper is wrong, you may fix it here and add a shuffled-queue test, e.g. `report_item_matches_play_order_under_shuffle`.

Done when: a written table lists every read site, with a verdict for each. Any follow-up tasks are registered. Workspace tests, fmt and clippy are clean.

## Brief

The file list is complete. Scope is fine: only docs and a task file changed, and there are no code edits outside loxia-core. But the audit's two deliverables, a verdict for every read site and registered follow-ups, are not met.

1. **MPRIS follow-up is missing.** `docs/15-play-order-consumer-audit.md` says `tasks/phase-10-polish/10-14-mpris-play-order-audit.md` was registered, but that file is not in the changed-file list. Either add it or remove the claim. Right now the doc states something the branch does not contain.

2. **`tasks/README.md` is not updated.** The work item says to register follow-up tasks in `tasks/README.md`, and it is not in the changed-file list. Add entries for 07-08 and the MPRIS task there.

3. **Rows 8, 9 and 10 have no verdicts.** "Unconfirmed" is not a verdict. The ban on changes outside core covers fixes, not reading code. Open `crates/loxia-tui/src/views/now_playing.rs`, `crates/loxia-tui/src/widgets/player_bar.rs` and the MPRIS worker in loxia-player. Record the exact symbol and whether each goes through `play_order`. Register a follow-up only for sites that are actually wrong.
   - Row 10 says the MPRIS code was "unlocated with certainty" yet gives a path, `crates/loxia-player/src/workers/mpris.rs`. Confirm the real location from the `mpris` grep. Separate verdicts for current track, track list, `CanGoNext` and `CanGoPrevious`.
   - Once those verdicts exist, rewrite `tasks/phase-07-views/07-08-queue-view-play-order-audit.md` as a fix task naming the offending lines, or drop it if both sites are correct. As written, it hands off the audit itself instead of a fix.

4. **The table is not per-site.** Row 4 ("track-advance / track-ended handling") names no function, and the table lists no raw `entries[` or `.entries.get(` hits anywhere. List every hit from those greps with file and symbol, including test code and any in-order iteration over `entries`, each with a verdict. If a grep returned nothing outside the helpers, say so explicitly.

## Status

Scaffolded by the delegated coding agent. TODO: implement.
