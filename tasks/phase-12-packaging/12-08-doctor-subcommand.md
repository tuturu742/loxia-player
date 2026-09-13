# 12-08 · Doctor subcommand

**Phase:** 12 — Packaging · **Agent:** E · **Size:** S
**Prerequisites:** `11-07`
**Reference:** `docs/11-packaging.md` §9

## Goal
`loxia-player --doctor` — a diagnostics command that prints and checks the whole environment. It is the
first thing to request in a bug report and it makes release QA far faster.

## Files
- `crates/loxia-player/src/doctor.rs`

## Specification

Runs **without entering the TUI**, prints plain text to stdout, and exits `0` when everything checks
out or `1` when any check fails.

```
loxia 0.1.0  (rustc 1.97.1, x86_64-unknown-linux-gnu)

 ✓ config          ~/.config/loxia-player/config.toml  (valid, mode 0600)
 ⚠ keybindings     2 conflicts
                     'a' -> queue_artist_only, instant_mix  (using instant_mix)
 ✓ libmpv          2.2.0  /usr/lib/x86_64-linux-gnu/libmpv.so.2
 ✓ audio devices   4 found, 1 bit-perfect capable
 ✓ terminal        kitty  204x52  graphics: kitty
 ✓ cache           ~/.cache/loxia-player  2.1 GB / 5.0 GB
 ✓ downloads       ~/.local/share/loxia-player/downloads  14.7 GB, 312 tracks
 ✓ server          Remote Emby Server  (Emby 4.8.11)  142 ms
 ✓ scrobbles       0 pending

2 warnings, 0 errors
```

Checks, in this order — each independent, so one failure does not prevent the rest from reporting:

| Check | Failure condition |
| :-- | :-- |
| config | unparseable, or world-readable on Unix |
| keybindings | any conflict from `KeyMap::validate` — this is the third required conflict surface, alongside the startup toast and the Settings badge |
| libmpv | not found or below mpv 0.35 |
| audio devices | enumeration fails, or none found |
| terminal | reports size and graphics protocol; never fails |
| cache / downloads | path not writable, or index corrupt |
| server | authentication or reachability failure, with the round-trip time |
| scrobbles | reports the pending count; never fails |

**Redaction.** No access token, no custom header value, and no stream URL appears in the output —
the whole point is that a user can paste it into a public issue. A test asserts this.

`--doctor --json` emits the same information as JSON for scripting.

`--doctor` implies `--no-audio` for device enumeration only when mpv is missing, so the command
still produces a full report on a machine where the audio engine cannot start.

## Acceptance
- `doctor_reports_all_checks`
- `doctor_exit_code_zero_when_healthy`
- `doctor_exit_code_one_on_any_error`
- `doctor_continues_after_a_failed_check` — a broken config still yields libmpv and terminal rows.
- `doctor_reports_keybinding_conflicts_with_actions`
- `doctor_output_contains_no_token`
- `doctor_output_contains_no_custom_header_values`
- `doctor_json_is_valid_and_matches_text`
- `doctor_works_without_libmpv` — reports the missing library as an error and still prints the rest.
- `doctor_does_not_enter_the_tui` — stdout is plain text and the terminal is never put into raw mode.

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 12's exit criteria in
`docs/08-roadmap.md` are met — the project is releasable.
