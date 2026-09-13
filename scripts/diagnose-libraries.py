#!/usr/bin/env python3
"""Diagnose multi-library browsing problems against a live Emby server.

loxia-player treats libraries in two different ways, and they can disagree:

  * `items::music_libraries()` keeps only views whose `CollectionType` is exactly `"music"`.
    It drives **both** the Folders tab's root list **and** the browsing scope: several music
    libraries means "no ParentId" (server-wide union), exactly one means "scope to that one".
  * `endpoints::discography` looks albums up by `AlbumArtistIds`/`ArtistIds` with **no** ParentId,
    so it should find an artist's albums wherever they live.

This prints what the server actually says for each, so a report like "album artists from both
libraries, but one library's albums and folders are missing" can be pinned to a cause instead of
guessed at.

Usage:

    python3 scripts/diagnose-libraries.py --url http://server:8096 --user NAME
    python3 scripts/diagnose-libraries.py --url ... --user ... --artist "Some Artist"

A server behind an authenticating proxy needs the same custom headers loxia-player is configured
with, or every request is rejected before Emby ever sees it:

    python3 scripts/diagnose-libraries.py --url ... --user ... \
        --header 'CF-Access-Client-Id: ...' --header 'CF-Access-Client-Secret: ...'

Header *values* are never printed back — only their names.

The password is prompted for (or taken from EMBY_PASS) and is never printed, written or passed on a
command line. Nothing is modified on the server — every request is a read.
"""

import argparse
import getpass
import http.client
import json
import os
import sys
import urllib.parse

AUTH = (
    'MediaBrowser Client="loxia-diagnose", Device="diagnose", '
    'DeviceId="loxia-diagnose-0001", Version="0.1.0"'
)
# Exactly the field list loxia-player's own ItemQuery sends.
FIELDS = (
    "Genres,DateCreated,MediaSources,UserData,ProductionYear,PremiereDate,"
    "Overview,ParentId,ArtistItems,AlbumArtists,ChildCount,RunTimeTicks"
)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", required=True, help="e.g. http://192.168.1.10:8096")
    ap.add_argument("--user", required=True)
    ap.add_argument(
        "--header",
        action="append",
        default=[],
        metavar="NAME: VALUE",
        help="an extra request header, repeatable — use the same ones configured under this "
        "server's custom headers, or a proxy will reject every request",
    )
    ap.add_argument(
        "--artist",
        help="an artist from the library whose albums do not load; "
        "otherwise one is sampled from each library",
    )
    args = ap.parse_args()

    base = args.url.rstrip("/")
    password = os.environ.get("EMBY_PASS") or getpass.getpass(f"password for {args.user}: ")

    extra = parse_headers(args.header)
    if extra:
        print(f"sending {len(extra)} custom header(s): {', '.join(sorted(extra))}")
    transport = Transport(base, extra)
    if not preflight(transport):
        return 1
    token, user_id = authenticate(transport, args.user, password)
    api = Api(transport, token, user_id)

    views = api.get(f"Users/{user_id}/Views")["Items"]
    rule("every view on this server, and how loxia-player classifies it")
    music = []
    for v in views:
        collection = v.get("CollectionType")
        counts_as_music = collection == "music"
        if counts_as_music:
            music.append(v)
        print(
            f"  {v['Name'][:34]:34} CollectionType={str(collection):14} "
            f"{'-> music library' if counts_as_music else '-> IGNORED by music_libraries()'}"
        )
    print()
    print(f"  music_libraries() returns {len(music)} librar{'y' if len(music) == 1 else 'ies'}")
    if len(music) == 1:
        print("  => browsing lists are SCOPED to that one library (ParentId set)")
    else:
        print("  => browsing lists span the whole server (no ParentId)")
    if len(music) != len([v for v in views if audio_ish(v)]):
        print("  !! some audio-looking views are not typed 'music' — see the list above")

    rule("per-library counts, and the union loxia-player actually requests")
    for endpoint in ("Artists", "Artists/AlbumArtists"):
        print(f"\n  {endpoint}:")
        for v in views:
            if not audio_ish(v):
                continue
            total = api.total(endpoint, ParentId=v["Id"], Recursive="true", UserId=user_id)
            print(f"     ParentId={v['Name'][:26]:26} total={total}")
        union = api.total(endpoint, Recursive="true", UserId=user_id)
        print(f"     {'no ParentId (what loxia asks for)':34} total={union}")

    print("\n  MusicAlbum:")
    for v in views:
        if not audio_ish(v):
            continue
        total = api.total(
            f"Users/{user_id}/Items",
            ParentId=v["Id"],
            IncludeItemTypes="MusicAlbum",
            Recursive="true",
        )
        print(f"     ParentId={v['Name'][:26]:26} total={total}")

    rule("does an artist's discography resolve? (the AlbumArtistIds / ArtistIds queries)")
    artists = pick_artists(api, user_id, views, args.artist)
    if not artists:
        print("  no artists found to test")
    for label, artist in artists:
        aid, name = artist["Id"], artist["Name"]
        print(f"\n  {label}: {name!r} (id={aid})")
        primary = api.total(
            f"Users/{user_id}/Items",
            IncludeItemTypes="MusicAlbum",
            Recursive="true",
            AlbumArtistIds=aid,
        )
        appears = api.total(
            f"Users/{user_id}/Items",
            IncludeItemTypes="MusicAlbum",
            Recursive="true",
            ArtistIds=aid,
        )
        tracks = api.total(
            f"Users/{user_id}/Items",
            IncludeItemTypes="Audio",
            Recursive="true",
            ArtistIds=aid,
        )
        print(f"     AlbumArtistIds -> {primary} albums   (loxia's ALBUMS section)")
        print(f"     ArtistIds      -> {appears} albums   (used for the APPEARS ON split)")
        print(f"     ArtistIds      -> {tracks} tracks")
        if primary == 0 and appears == 0:
            print("     !! this is the failure: the server returns no albums for this artist id")
            if tracks:
                # The tracks exist but aren't reachable from the artist id — say where they live.
                sample = api.get(
                    f"Users/{user_id}/Items",
                    IncludeItemTypes="Audio",
                    Recursive="true",
                    ArtistIds=aid,
                    Fields=FIELDS,
                    Limit=1,
                )["Items"]
                if sample:
                    t = sample[0]
                    print(
                        f"     but {tracks} tracks exist, e.g. {t.get('Name')!r} "
                        f"album={t.get('Album')!r} albumId={t.get('AlbumId')!r}"
                    )
                    print("     => the tracks are not linked to a MusicAlbum the artist id reaches")

    rule("what the Folders tab's root will list")
    for v in music:
        print(f"  {v['Name']}  (id={v['Id']})")
    if not music:
        print("  (nothing — no view is typed 'music')")

    print("\nDone. Paste this whole output back; nothing here contains your password or token.")
    return 0


def audio_ish(view) -> bool:
    """Views worth counting as possibly-musical: typed music, or untyped (mixed) content."""
    return view.get("CollectionType") in ("music", None)


def pick_artists(api, user_id, views, requested):
    if requested:
        found = api.get(
            "Artists", SearchTerm=requested, Recursive="true", UserId=user_id, Limit=5
        )["Items"]
        return [("requested", a) for a in found[:1]] or []
    out = []
    for v in views:
        if not audio_ish(v):
            continue
        items = api.get(
            "Artists/AlbumArtists",
            ParentId=v["Id"],
            Recursive="true",
            SortBy="SortName",
            UserId=user_id,
            Limit=1,
        )["Items"]
        if items:
            out.append((f"first album artist in {v['Name']!r}", items[0]))
    return out


def parse_headers(raw):
    """`Name: Value` pairs, as they appear in the config's own `custom_headers`."""
    out = {}
    for item in raw:
        name, sep, value = item.partition(":")
        if not sep:
            sys.exit(f"--header must be 'Name: Value', got {item!r}")
        out[name.strip()] = value.strip()
    return out


class Transport:
    """Raw `http.client`, deliberately not `urllib`.

    `urllib.request` runs every header name through `str.capitalize()`, so a
    `CF-Access-Client-Id` service token goes out as `Cf-access-client-id`. Field names are
    case-insensitive per RFC 7230 and a compliant proxy accepts either, but when the proxy is what
    is rejecting you that is exactly the variable worth removing rather than arguing about.
    """

    def __init__(self, base, extra=None):
        parsed = urllib.parse.urlsplit(base)
        self.https = parsed.scheme != "http"
        self.host = parsed.hostname
        self.port = parsed.port
        self.prefix = parsed.path.rstrip("/")
        self.extra = extra or {}

    def _connect(self):
        cls = http.client.HTTPSConnection if self.https else http.client.HTTPConnection
        return cls(self.host, self.port, timeout=30)

    def request(self, method, path, body=None, headers=None):
        conn = self._connect()
        try:
            conn.request(method, f"{self.prefix}{path}", body=body, headers=headers or {})
            response = conn.getresponse()
            payload = response.read()
            return response.status, dict(response.getheaders()), payload
        finally:
            conn.close()

    def describe_failure(self, status, headers, body):
        """Enough to tell a proxy rejection from an Emby one, which is the whole question."""
        lines = [f"     HTTP {status}"]
        interesting = [
            "server",
            "cf-ray",
            "cf-mitigated",
            "www-authenticate",
            "location",
            "content-type",
        ]
        for name in interesting:
            for key, value in headers.items():
                if key.lower() == name:
                    lines.append(f"     {key}: {value[:120]}")
        snippet = body.decode("utf-8", "replace").strip().replace("\n", " ")[:300]
        if snippet:
            lines.append(f"     body: {snippet}")
        via_cloudflare = any(k.lower().startswith("cf-") for k in headers)
        if via_cloudflare and status in (301, 302, 403):
            lines.append(
                "     => this is Cloudflare, not Emby: the service token was not accepted. "
                "Check the header names and values against the ones in your loxia-player config."
            )
        return "\n".join(lines)


class Api:
    def __init__(self, transport, token, user_id):
        self.t, self.token, self.user_id = transport, token, user_id

    def get(self, path, **params):
        params.setdefault("api_key", self.token)
        query = urllib.parse.urlencode(params)
        headers = {"Accept": "application/json", **self.t.extra}
        status, _, body = self.t.request("GET", f"/emby/{path}?{query}", headers=headers)
        if status != 200:
            return {"Items": [], "TotalRecordCount": f"HTTP {status}"}
        try:
            return json.loads(body)
        except json.JSONDecodeError:
            return {"Items": [], "TotalRecordCount": "not JSON"}

    def total(self, path, **params):
        params["Limit"] = 0
        return self.get(path, **params).get("TotalRecordCount", "?")


def preflight(transport):
    """Can we reach Emby at all through whatever sits in front of it?"""
    rule("reachability")
    status, headers, body = transport.request(
        "GET", "/emby/System/Info/Public", headers={"Accept": "application/json", **transport.extra}
    )
    if status == 200:
        try:
            info = json.loads(body)
            print(f"  reached Emby {info.get('Version')!r} ({info.get('ServerName')!r})")
        except json.JSONDecodeError:
            print("  got HTTP 200 but not JSON — something is intercepting the response")
        return True
    print("  could not reach Emby's public endpoint:")
    print(transport.describe_failure(status, headers, body))
    return False


def authenticate(transport, user, password):
    payload = json.dumps({"Username": user, "Pw": password}).encode()
    headers = {
        "Content-Type": "application/json",
        "X-Emby-Authorization": AUTH,
        **transport.extra,
    }
    status, resp_headers, body = transport.request(
        "POST", "/emby/Users/AuthenticateByName", body=payload, headers=headers
    )
    if status != 200:
        print("\nauthentication failed:")
        print(transport.describe_failure(status, resp_headers, body))
        if status in (401,):
            print("     => Emby rejected the username or password.")
        sys.exit(1)
    data = json.loads(body)
    return data["AccessToken"], data["User"]["Id"]


def rule(title):
    print(f"\n{'=' * 78}\n{title}\n{'=' * 78}")


if __name__ == "__main__":
    sys.exit(main())
