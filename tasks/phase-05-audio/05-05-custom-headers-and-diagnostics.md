# 05-05 · Custom headers and diagnostics

**Phase:** 05 — Audio MVP · **Agent:** C · **Size:** S
**Prerequisites:** `05-04`
**Reference:** `docs/05-audio-engine.md` §§3, 8

## Goal
Pass the user's custom HTTP headers to mpv, and make a missing libmpv produce a helpful message.
Without the first, every reverse-proxy and zero-trust deployment fails to play anything.

## Files
- `crates/loxia-audio/src/mpv/handle.rs` (extend)
- `crates/loxia-player/src/main.rs` (extend)

## Specification

**Headers.** mpv performs its own HTTP request and does not share the `EmbyClient`'s header map, so
`AudioCommand::Load.headers` must be handed to mpv as `http-header-fields`:
```
http-header-fields = "Name: Value,Other: Value"
```
Set it **per `loadfile`** as a file-local option, not globally — a global setting would leak one
server profile's headers to another after a profile switch.

Encoding rules:
- Each entry is `Name: Value`; entries are comma-joined.
- A value containing a comma or a `"` is quoted and inner quotes escaped, per mpv's option syntax.
- A header whose name or value contains a newline is **dropped with a `warn!`** — header injection
  through a config file is a real, if unlikely, attack surface.
- An empty header list omits the option entirely rather than setting an empty string.

Also set `user-agent = loxia/<version>` globally.

**Diagnostics.** When `MpvEngine::new` returns `LibraryNotFound`, `main` prints to stderr **after
restoring the terminal**:
```
loxia requires libmpv, which was not found.

  <platform hint>

See https://<repo>#installing-mpv for details.
```
and exits with status 1. No backtrace, no panic — the user's problem is a missing package, and the
message should say so.

`--no-audio` substitutes `MockEngine`, so the app remains usable for browsing on a machine with no
mpv. Mention this in the message as a fallback.

## Acceptance
- `header_encoding_table` — plain, value with comma, value with quote, empty list, single header.
- `newline_in_header_is_dropped_with_warning`
- `headers_are_set_per_loadfile_not_globally` — against the recording stub from `05-03`.
- `empty_headers_omits_option`
- `user_agent_is_set`
- Integration: a local `tiny_http` server asserting the custom header arrived on mpv's request
  (behind `mpv-tests`).
- Manual, pasted into the PR: rename libmpv temporarily and confirm the message is the one above,
  the shell is usable, and the exit code is 1. Confirm `--no-audio` starts successfully.

## Done when
The global DoD in `tasks/README.md` is satisfied.
