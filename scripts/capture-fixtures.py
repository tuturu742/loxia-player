#!/usr/bin/env python3
"""Captures redacted Emby API fixtures for loxia-emby's test suite (task 02-01).

Usage:
    1. Authenticate once, writing a local session file with 0600 permissions:

           python3 scripts/capture-fixtures.py login <base-url>
           (prompts for username and password; never pass them as arguments,
            where they would land in shell history)

    2. Capture fixtures using the saved session:

           python3 scripts/capture-fixtures.py capture <base-url> <music-library-id>

Only the standard library is used (urllib, json) so this runs with no extra dependencies —
`curl`+`jq` was the original plan, but `jq` is not reliably available in every environment this
might run in.

Redaction: every fixture has the real user id, server id, private IP addresses, and on-disk file
paths replaced before being written. This is a safety net, not the only check — the CI fixture
secret scan in ci.yml is the enforced gate.
"""

import getpass
import json
import os
import re
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request

SESSION_FILE = os.path.expanduser("~/.loxia-capture-session.json")
FIXTURES_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "crates", "loxia-emby", "tests", "fixtures",
)

PRIVATE_IP = re.compile(r"\b(?:10\.|192\.168\.|172\.(?:1[6-9]|2\d|3[01])\.)\d{1,3}\.\d{1,3}\b")


def emby_call(base, token, method, path, params=None, body=None):
    url = base.rstrip("/") + "/emby" + path
    if params:
        url += "?" + urllib.parse.urlencode(params, doseq=True)
    headers = {"X-Emby-Token": token}
    data = json.dumps(body).encode() if body is not None else None
    if data:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            raw = resp.read()
            return resp.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw) if raw else None
        except Exception:
            return e.code, raw


def redact(obj, user_id, server_id):
    s = json.dumps(obj)
    if user_id:
        s = s.replace(user_id, "0" * len(user_id))
    if server_id:
        s = s.replace(server_id, "f" * len(server_id))
    s = PRIVATE_IP.sub("203.0.113.10", s)
    s = re.sub(r'"Path"\s*:\s*"[^"]*"', '"Path": "/redacted/path.flac"', s)
    return json.loads(s)


def cmd_login(base):
    device_auth_header = (
        'MediaBrowser Client="loxia-probe", Device="capture-script", '
        'DeviceId="loxia-capture-device-0001", Version="0.1.0"'
    )
    username = input("Emby username: ")
    password = getpass.getpass("Emby password: ")
    req = urllib.request.Request(
        base.rstrip("/") + "/emby/Users/AuthenticateByName",
        data=json.dumps({"Username": username, "Pw": password}).encode(),
        headers={"Content-Type": "application/json", "X-Emby-Authorization": device_auth_header},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=10) as resp:
        data = json.loads(resp.read())
    session = {
        "base": base,
        "user_id": data["User"]["Id"],
        "access_token": data["AccessToken"],
        "server_id": data.get("ServerId", ""),
    }
    with open(SESSION_FILE, "w") as f:
        json.dump(session, f)
    os.chmod(SESSION_FILE, stat.S_IRUSR | stat.S_IWUSR)
    print(f"session saved to {SESSION_FILE} (mode 0600)")


def cmd_capture(base, library_id):
    with open(SESSION_FILE) as f:
        sess = json.load(f)
    token, uid, server_id = sess["access_token"], sess["user_id"], sess["server_id"]
    os.makedirs(FIXTURES_DIR, exist_ok=True)

    def save(name, obj):
        with open(os.path.join(FIXTURES_DIR, name), "w") as f:
            json.dump(redact(obj, uid, server_id), f, indent=2)
            f.write("\n")
        print(f"wrote {name}")

    default_fields = (
        "Genres,DateCreated,MediaSources,UserData,ProductionYear,PremiereDate,"
        "Overview,ParentId,ArtistItems,AlbumArtists,ChildCount,RunTimeTicks"
    )

    _, views = emby_call(base, token, "GET", f"/Users/{uid}/Views")
    save("views.json", views)

    _, artists = emby_call(
        base, token, "GET", "/Artists",
        {"ParentId": library_id, "Recursive": "true", "SortBy": "SortName", "Limit": 10},
    )
    save("artists.json", artists)

    _, album_artists = emby_call(
        base, token, "GET", "/Artists/AlbumArtists",
        {"ParentId": library_id, "Recursive": "true", "Limit": 10},
    )
    save("album_artists.json", album_artists)

    _, genres = emby_call(base, token, "GET", "/MusicGenres", {"ParentId": library_id, "Limit": 10})
    save("genres.json", genres)

    _, favorites = emby_call(
        base, token, "GET", f"/Users/{uid}/Items",
        {"Filters": "IsFavorite", "IncludeItemTypes": "MusicArtist,MusicAlbum,Audio", "Recursive": "true"},
    )
    save("favorites.json", favorites)

    print("done — see docs/12-decisions.md §10 for the artist/album/track-specific captures")


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ("login", "capture"):
        print(__doc__)
        sys.exit(2)
    if sys.argv[1] == "login":
        cmd_login(sys.argv[2])
    else:
        library_id = sys.argv[3] if len(sys.argv) > 3 else None
        if not library_id:
            print("usage: capture-fixtures.py capture <base-url> <music-library-id>")
            sys.exit(2)
        cmd_capture(sys.argv[2], library_id)


if __name__ == "__main__":
    main()
