# 02-13 · Probe example

**Phase:** 02 — Emby client · **Agent:** B · **Size:** S
**Prerequisites:** `02-06`, `02-08`, `02-11`
**Reference:** `docs/08-roadmap.md` (phase 02 exit criteria)

## Goal
A small CLI that exercises the whole client against the live server. It is the phase-02 exit gate and
stays useful afterwards as the fastest way to check a server-side assumption.

## Files
- `crates/loxia-emby/examples/probe.rs`

## Specification

```
cargo run -p loxia-emby --example probe -- <subcommand>
```
Credentials come from `LOXIA_SERVER`, `LOXIA_TOKEN`, `LOXIA_USER` in the environment — **never** from
arguments, where they would enter shell history. Missing variables produce a clear message and
exit 2.

Subcommands:

| Subcommand | Output |
| :-- | :-- |
| `libraries` | each music library's name and id |
| `artists [--limit N]` | artist names with album and track counts |
| `discography <artist-id>` | the ALBUMS / APPEARS ON split exactly as the UI will group it. For diagnostic richness, additionally fetches each appears-on album's tracklist (a handful of extra requests — appears-on lists are small) and prints how many tracks feature the artist, verifying the `a`/`A` filter logic too. The production UI does **not** do this eagerly; see `docs/03-emby-api.md` §4 |
| `album <album-id>` | the full tracklist with disc/track numbers, duration, and codec |
| `search <query>` | the three result sections with counts |
| `playlists` | playlists, then the first playlist's entries **with their entry ids** |
| `lyrics <track-id>` | whether a lyric stream was found, its format, and the first 5 parsed lines |
| `stream <track-id>` | the built URL for each quality profile, **token redacted** |

Output is plain text for humans, not JSON. Add `--json` only if a later task needs it.

`discography` is the important one: it is how a human confirms the appears-on split is right, which
is exactly what phase 02's exit criteria ask for.

**No token may appear in any output**, including the `stream` subcommand and any error message.

## Acceptance
- Manual, with the results pasted into the PR description:
  - `libraries` lists the real music library.
  - `discography <id>` for an artist with both primary albums and compilation appearances shows a
    correct split, and the appears-on entries show only that artist's track counts.
  - `lyrics <id>` for a track with an `.lrc` sidecar prints 5 timestamped lines.
  - `playlists` shows non-empty entry ids.
- `probe_output_never_contains_token` — an automated test running the `stream` subcommand's
  formatting logic with a known token and asserting it is absent from the output.
- Missing environment variables exit 2 with a helpful message.

## Done when
The global DoD in `tasks/README.md` is satisfied, and phase 02's exit criteria in
`docs/08-roadmap.md` are demonstrably met.
