# 11-03 · Server profiles

**Phase:** 11 — Settings · **Agent:** A · **Size:** M
**Prerequisites:** `11-01`
**Reference:** `docs/02-data-model.md` §8, `docs/03-emby-api.md` §2

## Goal
Add, edit, remove, and switch between Emby servers, including the custom-header editor that
reverse-proxy deployments need.

## Files
- `crates/loxia-tui/src/views/settings.rs` (extend)

## Specification

The Servers section lists profiles with the active one marked `•`, and offers add, edit, remove, and
switch.

**Profile editor fields:** `name`, `url`, `username` + `password` (for login), and a custom-header
sub-editor. `user_id`, `access_token`, and `device_id` are **derived, not typed** — they come from
authentication. Presenting a token field invites users to paste one from another client, which
produces a session that behaves strangely.

**`Test connection`** is an `Action` control: authenticate, then `GET /System/Info/Public`, and
report `connected to <server name> (v<version>)` or the mapped `EmbyError` message. It must be
possible to test **before** saving, so a user can iterate on a URL or a header without committing a
broken profile.

**Custom headers** are a `name: value` list with add and remove. Reserved names — `authorization`,
`x-emby-authorization`, `host`, `content-length` — are rejected inline with
`{name} is set by loxia and cannot be overridden`. Values are masked like secrets, since they are
frequently access tokens.

**Switching servers** is disruptive and must be explicit: a `Confirm` warning that the queue and
session will be cleared. On confirm: stop playback, clear the queue and history, persist the outgoing
session under its own server id, reconnect, and reseed the columns.

**Removing** the active profile requires switching first; the control is disabled with the reason
shown rather than hidden.

**Removing any profile** offers to delete its cached files and downloads, defaulting to **no** —
downloads are expensive to reacquire and a user removing a profile is usually reorganising, not
discarding.

`device_id` is generated once per profile at creation and never edited.

## Acceptance
- `token_is_not_an_editable_field`
- `test_connection_works_before_save`
- `test_connection_reports_server_name_and_version`
- `test_connection_maps_error_to_message` — table test over unauthorized, offline, and invalid URL.
- `reserved_header_rejected_inline`
- `header_values_masked`
- `switching_requires_confirmation`
- `switching_clears_queue_and_persists_outgoing_session`
- `removing_active_profile_is_disabled_with_reason`
- `removing_profile_offers_data_deletion_defaulting_to_no`
- `device_id_generated_once_per_profile`
- `servers_section_snapshot`, `_editor`, `_headers`

## Done when
The global DoD in `tasks/README.md` is satisfied.
