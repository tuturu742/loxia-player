//! A small CLI that exercises the whole `loxia-emby` client against a live server. This is the
//! phase-02 exit gate and stays useful afterwards as the fastest way to check a server-side
//! assumption.
//!
//! ```text
//! LOXIA_SERVER=http://host:8096 LOXIA_USER=<user-guid> LOXIA_TOKEN=<token> \
//!     cargo run -p loxia-emby --example probe -- <subcommand>
//! ```
//!
//! Credentials come from the environment, **never** from arguments, where they would enter shell
//! history.

use std::collections::BTreeMap;
use std::time::Duration;

use clap::{Parser, Subcommand};

use loxia_core::config::{QualityProfile, ServerConfig, TargetCodec};
use loxia_core::model::{Artist, ItemId, Lyrics, PlaylistId};
use loxia_emby::client::EmbyClient;
use loxia_emby::endpoints::{discography, items, lyrics, playback, playlists, search};
use loxia_emby::query::Page;
use loxia_emby::stream::StreamUrl;

#[derive(Parser)]
#[command(
    name = "probe",
    about = "Exercise the loxia-emby client against a live server"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Each music library's name and id.
    Libraries,
    /// Artist names with album and track counts (one discography fetch per artist, so this is
    /// capped by --limit).
    Artists {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// The ALBUMS / APPEARS ON split exactly as the UI will group it.
    Discography { artist_id: String },
    /// The full tracklist with disc/track numbers, duration, and codec.
    Album { album_id: String },
    /// The three result sections with counts.
    Search { query: String },
    /// Playlists, then the first playlist's entries with their entry ids.
    Playlists,
    /// Whether a lyric stream was found, its format, and the first 5 parsed lines.
    Lyrics { track_id: String },
    /// The built URL for each quality profile, token redacted.
    Stream { track_id: String },
}

fn require_env(name: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!(
                "error: missing required environment variable {name}\n\
                 set LOXIA_SERVER, LOXIA_USER, and LOXIA_TOKEN before running probe"
            );
            std::process::exit(2);
        }
    }
}

fn client_from_env() -> EmbyClient {
    let cfg = ServerConfig {
        id: "probe".to_string(),
        name: "probe".to_string(),
        url: require_env("LOXIA_SERVER"),
        user_id: require_env("LOXIA_USER"),
        access_token: require_env("LOXIA_TOKEN"),
        device_id: uuid::Uuid::new_v4().to_string(),
        custom_headers: BTreeMap::new(),
        server_id: String::new(),
        fallbacks: Vec::new(),
    };
    EmbyClient::new(&cfg).unwrap_or_else(|e| {
        eprintln!("error: could not build client: {e}");
        std::process::exit(2);
    })
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// A minimal `Artist` carrying just an id — `discography()` only ever reads `artist.id`, so this
/// is enough to drive it from a bare CLI argument without a separate "fetch one artist" endpoint.
fn placeholder_artist(id: ItemId) -> Artist {
    Artist {
        id,
        name: String::new(),
        sort_name: String::new(),
        album_count: 0,
        track_count: 0,
        genres: Vec::new(),
        is_favorite: false,
        image: None,
        overview: None,
    }
}

/// The `stream` subcommand's output, factored out so it can be tested without a live server —
/// `StreamUrl`'s own `Display` redacts `api_key`, so this can never leak a token.
fn format_stream_output(client: &EmbyClient, track_id: &ItemId) -> String {
    let mut out = String::new();
    for (label, profile, codec) in [
        ("Direct", QualityProfile::Direct, TargetCodec::Mp3),
        (
            "TranscodeHigh (mp3)",
            QualityProfile::TranscodeHigh,
            TargetCodec::Mp3,
        ),
        (
            "TranscodeMed (aac)",
            QualityProfile::TranscodeMed,
            TargetCodec::Aac,
        ),
        (
            "TranscodeLow (opus)",
            QualityProfile::TranscodeLow,
            TargetCodec::Opus,
        ),
    ] {
        let url = StreamUrl::build(client, track_id, profile, codec);
        out.push_str(&format!("{label}: {url}\n"));
    }
    out
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = client_from_env();

    match cli.command {
        Command::Libraries => {
            for lib in items::music_libraries(&client).await? {
                println!("{}\t{}", lib.name, lib.id);
            }
        }
        Command::Artists { limit } => {
            let page =
                items::artists(&client, Some(&first_library(&client).await?), Page::FIRST).await?;
            for artist in page.items.into_iter().take(limit) {
                let disco = discography::discography(&client, &artist).await?;
                let track_count: u32 = disco.primary.iter().map(|a| a.track_count).sum();
                println!(
                    "{}\talbums={}\tappears_on={}\ttracks={}",
                    artist.name,
                    disco.primary.len(),
                    disco.appears_on.len(),
                    track_count
                );
            }
        }
        Command::Discography { artist_id } => {
            let artist = placeholder_artist(ItemId::from(artist_id));
            let disco = discography::discography(&client, &artist).await?;
            println!("ALBUMS ({})", disco.primary.len());
            for album in &disco.primary {
                println!(
                    "  {}\t{}\t{:?}",
                    album.name,
                    album.year.map(|y| y.to_string()).unwrap_or_default(),
                    album.id
                );
            }
            println!("APPEARS ON ({})", disco.appears_on.len());
            for album in &disco.appears_on {
                let tracks = items::album_tracks(&client, &album.id).await?;
                let matching = tracks
                    .iter()
                    .filter(|t| t.artist_ids.contains(&artist.id))
                    .count();
                println!(
                    "  {}\t{}\t{} of {} tracks feature this artist",
                    album.name,
                    album.year.map(|y| y.to_string()).unwrap_or_default(),
                    matching,
                    tracks.len()
                );
            }
        }
        Command::Album { album_id } => {
            let tracks = items::album_tracks(&client, &ItemId::from(album_id)).await?;
            for t in tracks {
                println!(
                    "{}.{}\t{}\t{}\t{}",
                    t.disc_number.unwrap_or(1),
                    t.track_number.unwrap_or(0),
                    t.name,
                    format_duration(t.duration),
                    t.format.codec.label()
                );
            }
        }
        Command::Search { query } => {
            let results = search::search(&client, &query, 50).await?;
            println!("Artists ({})", results.artists.len());
            for a in &results.artists {
                println!("  {}", a.name);
            }
            println!("Albums ({})", results.albums.len());
            for a in &results.albums {
                println!("  {}", a.name);
            }
            println!("Tracks ({})", results.tracks.len());
            for t in &results.tracks {
                println!("  {}", t.name);
            }
        }
        Command::Playlists => {
            let page = playlists::list(&client, Page::FIRST).await?;
            for p in &page.items {
                println!("{}\t{}\t{} tracks", p.name, p.id, p.track_count);
            }
            if let Some(first) = page.items.first() {
                println!("--- entries for {} ---", first.name);
                // `Playlist.id` is a plain `ItemId` (needed for MediaItem's uniform id() accessor
                // across every variant); the playlist-entry endpoints want the distinct
                // `PlaylistId` type, so the conversion happens here at the call site.
                let playlist_id = PlaylistId::from(first.id.as_str());
                for entry in playlists::items(&client, &playlist_id).await? {
                    println!("{}\t{}", entry.entry_id, entry.track.name);
                }
            }
        }
        Command::Lyrics { track_id } => {
            let item = ItemId::from(track_id);
            let source = playback::playback_info(&client, &item).await?;
            match source.lyric_stream {
                None => println!("no lyric stream found"),
                Some(r) => {
                    println!("found: format={:?} index={}", r.format, r.stream_index);
                    match lyrics::fetch(&client, &item, &r).await? {
                        Lyrics::Synced(lines) => {
                            for line in lines.iter().take(5) {
                                println!("[{}] {}", format_duration(line.at), line.text);
                            }
                        }
                        Lyrics::Unsynced(lines) => {
                            for line in lines.iter().take(5) {
                                println!("{line}");
                            }
                        }
                    }
                }
            }
        }
        Command::Stream { track_id } => {
            print!("{}", format_stream_output(&client, &ItemId::from(track_id)));
        }
    }

    Ok(())
}

async fn first_library(client: &EmbyClient) -> anyhow::Result<ItemId> {
    let libraries = items::music_libraries(client).await?;
    libraries
        .into_iter()
        .next()
        .map(|l| l.id)
        .ok_or_else(|| anyhow::anyhow!("no music library found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> EmbyClient {
        let cfg = ServerConfig {
            id: "probe".to_string(),
            name: "probe".to_string(),
            url: "http://host:8096".to_string(),
            user_id: "user-1".to_string(),
            access_token: "SECRET-PROBE-TOKEN".to_string(),
            device_id: "dev".to_string(),
            custom_headers: BTreeMap::new(),
        };
        EmbyClient::new(&cfg).unwrap()
    }

    #[test]
    fn probe_output_never_contains_token() {
        let output = format_stream_output(&client(), &ItemId::from("t1"));
        assert!(!output.contains("SECRET-PROBE-TOKEN"));
        assert!(output.contains("REDACTED"));
    }
}
