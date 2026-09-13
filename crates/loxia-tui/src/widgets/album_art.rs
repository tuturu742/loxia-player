//! Terminal graphics album art rendering (`docs/07-ui-spec.md` §11).
//!
//! Three pieces, kept deliberately separate:
//! - [`detect_renderer`] resolves which graphics protocol to use, once, at startup.
//! - [`ArtCache`] holds already-encoded [`Protocol`] values, one per `(item, cell size)` — the
//!   expensive resize/encode step, done at most once per size, never per frame.
//! - [`render`] only ever reads the cache and blits (`Image::new(protocol)`, a stateless,
//!   render-time-cheap widget) or draws the placeholder; it never decodes or encodes anything
//!   itself, and it is the one place that decides — by finding nothing cached — that a fetch is
//!   needed, returning that as an effect rather than doing any I/O of its own.
//!
//! **Not wired into the real running app yet.** `crates/loxia-player/src/runtime.rs`'s own `draw()` is
//! still a placeholder stub (its own doc comment defers "wiring the real widget tree" to task
//! `10-04`, which needs it anyway to get a live `HitMap`), so nothing calls this module's `render`
//! for real today. This task builds the complete, tested component; `10-04` (or a dedicated
//! follow-up) is what threads an `ArtCache` through `render::draw` → `views::miller::render` →
//! `widgets::inspector::render`'s placeholder call site. See `docs/12-decisions.md`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use image::DynamicImage;
use loxia_core::config::ArtProtocol;
use loxia_core::effect::{Effect, NetEffect};
use loxia_core::model::{Album, Artist, ImageSize, ItemId, MediaItem, Track};
use loxia_core::theme::{Role, Theme};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect, Size};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};

use crate::style;

/// Bounds `Picker::from_query_stdio`'s real terminal round-trip.
///
/// Deliberately **longer** than `ratatui-image`'s own 2 s stdin budget, and so longer than the
/// 100 ms this originally used. The query thread cannot be cancelled: `query_stdio_capabilities`
/// loops on a blocking `io::stdin().read()` until the terminal answers its Device Status Report.
/// Abandoning it does not stop it — it stays parked on stdin and races the real input reader for
/// every keystroke afterwards. At 100 ms that happened to *any* terminal answering slower than
/// 100 ms (easily an ssh session or a loaded machine), and the symptom is the whole keyboard going
/// intermittently dead. Waiting past the library's own timeout means the thread is only ever
/// abandoned when the terminal never answers at all, which is the one case nothing can rescue.
/// A terminal that answers promptly still returns in milliseconds — `recv_timeout` yields as soon
/// as the value lands — so this costs nothing in the normal case (`docs/12-decisions.md`).
const DETECT_TIMEOUT: Duration = Duration::from_millis(2500);

/// "An LRU of 16" (`docs/07-ui-spec.md` §11).
const CACHE_CAPACITY: usize = 16;

/// The resolved outcome of protocol detection/forcing. `Off` means art is never rendered at all —
/// always the placeholder, and no fetch is ever requested for it either (there would be nothing
/// to do with the bytes).
pub enum ArtRenderer {
    Off,
    Picker(Picker),
}

/// Decoded images awaiting encoding, keyed by item id — the hand-off from `workers::network` (which
/// does the HTTP fetch and the CPU-bound decode, off the render thread) to this module (which owns
/// the terminal-protocol encoding and the cache).
///
/// Deliberately *not* routed through `Action`/`AppState`: a `DynamicImage` is render-layer data, and
/// `loxia-core`'s actions are plain serialisable values with no image dependency at all. The worker
/// still emits `DataAction::ImageLoaded` alongside, purely so the runtime knows to redraw.
pub type ArtStore = Arc<Mutex<HashMap<ItemId, DynamicImage>>>;

/// Everything `render` needs beyond the frame itself, bundled so threading art support through
/// `render::draw` → each view is one parameter rather than three.
pub struct Art<'a> {
    pub renderer: &'a ArtRenderer,
    pub cache: &'a mut ArtCache,
    pub store: &'a ArtStore,
}

/// Owns the three pieces an [`Art`] borrows, so a caller with no real pipeline (every render test,
/// and the final frame drawn during shutdown) can produce one in a line. `ArtRenderer::Off` means
/// the placeholder is drawn and no fetch is ever requested.
pub struct ArtOff {
    renderer: ArtRenderer,
    cache: ArtCache,
    store: ArtStore,
}

impl Default for ArtOff {
    fn default() -> Self {
        ArtOff {
            renderer: ArtRenderer::Off,
            cache: ArtCache::new(),
            store: ArtStore::default(),
        }
    }
}

impl ArtOff {
    pub fn art(&mut self) -> Art<'_> {
        Art {
            renderer: &self.renderer,
            cache: &mut self.cache,
            store: &self.store,
        }
    }
}

/// Real detection: `ui.album_art_protocol` forces a choice; `Auto` queries the terminal, bounded
/// by [`DETECT_TIMEOUT`]. See [`detect_renderer_with`] for the testable core.
pub fn detect_renderer(forced: ArtProtocol) -> ArtRenderer {
    detect_renderer_with(forced, || {
        Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks())
    })
}

/// `query` stands in for the real terminal round-trip: a test can hand a closure that never
/// returns, to prove the timeout fallback, or one that panics, to prove a forced protocol never
/// calls it at all. Runs `query` on its own thread so a terminal that never answers can never
/// block the caller past [`DETECT_TIMEOUT`] — the spawned thread itself is simply abandoned in
/// that case, which is why that timeout must outlast the query's own (see its doc comment).
pub(crate) fn detect_renderer_with<F>(forced: ArtProtocol, query: F) -> ArtRenderer
where
    F: FnOnce() -> Picker + Send + 'static,
{
    match forced {
        ArtProtocol::Off => ArtRenderer::Off,
        ArtProtocol::Auto => {
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                let _ = tx.send(query());
            });
            let picker = rx
                .recv_timeout(DETECT_TIMEOUT)
                .unwrap_or_else(|_| Picker::halfblocks());
            ArtRenderer::Picker(picker)
        }
        forced_protocol => {
            // A forced, non-`Auto`, non-`Off` protocol skips terminal I/O entirely — even the
            // bounded query above still touches a real terminal, and a sufficiently broken one
            // could misbehave in ways `recv_timeout` doesn't fully insure against. A safe default
            // font size, with the protocol pinned to whatever was forced, needs none of that.
            let mut picker = Picker::halfblocks();
            picker.set_protocol_type(match forced_protocol {
                ArtProtocol::Kitty => ProtocolType::Kitty,
                ArtProtocol::Sixel => ProtocolType::Sixel,
                ArtProtocol::Halfblocks => ProtocolType::Halfblocks,
                ArtProtocol::Auto | ArtProtocol::Off => unreachable!("handled above"),
            });
            ArtRenderer::Picker(picker)
        }
    }
}

/// Encodes a decoded image to `cell`'s exact size using `picker`'s own protocol — the expensive
/// step [`ArtCache`] exists to pay at most once per `(item, cell size)`. `None` only if
/// `ratatui-image` itself rejects the image (malformed dimensions).
pub fn encode(picker: &Picker, image: DynamicImage, cell: (u16, u16)) -> Option<Protocol> {
    picker
        .new_protocol(image, Size::new(cell.0, cell.1), Resize::Fit(None))
        .ok()
}

/// Already-encoded art, one entry per `(item, cell_width, cell_height)` — resizing/encoding per
/// frame is the single most expensive thing a TUI can do; without this, the terminal becomes
/// unusable. Capped at [`CACHE_CAPACITY`], oldest inserted evicted first.
pub struct ArtCache {
    entries: HashMap<(ItemId, u16, u16), Protocol>,
    order: VecDeque<(ItemId, u16, u16)>,
}

impl Default for ArtCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtCache {
    pub fn new() -> Self {
        ArtCache {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&self, id: &ItemId, cell: (u16, u16)) -> Option<&Protocol> {
        self.entries.get(&(id.clone(), cell.0, cell.1))
    }

    pub fn insert(&mut self, id: ItemId, cell: (u16, u16), protocol: Protocol) {
        let key = (id, cell.0, cell.1);
        if !self.entries.contains_key(&key) {
            self.order.push_back(key.clone());
            if self.order.len() > CACHE_CAPACITY
                && let Some(oldest) = self.order.pop_front()
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, protocol);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drops every cached size for `id` — the state half of "on resize, tab change, or track
    /// change, issue the protocol's delete before drawing the replacement"
    /// (`docs/07-ui-spec.md` §11). The other half — actually writing a Kitty/iTerm2 delete escape
    /// sequence to the real terminal, outside the normal `Frame` render cycle — needs a live
    /// terminal this state-only cache has no way to reach or verify; see `docs/12-decisions.md`.
    pub fn evict_item(&mut self, id: &ItemId) {
        self.order.retain(|k| &k.0 != id);
        self.entries.retain(|k, _| &k.0 != id);
    }
}

/// Which pane is asking — the two sizing rules `docs/07-ui-spec.md` §11 gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtSizeContext {
    Inspector,
    Zen,
}

/// The HTTP-level fetch size for art drawn into `rows` terminal rows. Keyed off the area actually
/// being filled rather than the calling context: the Now Playing pane is `Inspector`, but since art
/// now scales to the whole pane it is routinely far larger than the 64px `Thumb` that context used
/// to request — and a 64px asset stretched over twenty rows is exactly the "images are too small /
/// blocky" the field reported (`docs/12-decisions.md`).
fn fetch_size_for(rows: u16) -> ImageSize {
    if rows > THUMB_MAX_ROWS {
        ImageSize::Large
    } else {
        ImageSize::Thumb
    }
}

/// Above this many rows a `Thumb` (64px) would visibly upscale, so the 600px asset is fetched.
const THUMB_MAX_ROWS: u16 = 8;

/// The square-ish art area within `pane`: as many rows as fit, width twice
/// the row count (cells are roughly 1:2, so square pixel art needs twice as many columns as
/// rows), clipped to `pane`'s own width.
pub fn art_rect(_ctx: ArtSizeContext, pane: Rect) -> Rect {
    // Scales to whatever the pane actually offers rather than a fixed cap: a terminal cell is
    // roughly twice as tall as it is wide, so a square cover needs `2 * rows` columns. Taking the
    // larger of the two constraints kept art stuck at a dozen rows on a big terminal, which is what
    // made covers look postage-stamp-sized (`docs/12-decisions.md`).
    let rows = pane.height.min(pane.width / 2);
    let cols = rows * 2;
    // Centred horizontally in the pane; art pinned hard left looked accidental beside centred text.
    let x = pane.x + (pane.width.saturating_sub(cols)) / 2;
    Rect::new(x, pane.y, cols, rows)
}

/// The track → album → artist fallback chain, resolved by the caller with the ids it already
/// holds. Returns `(id, tag)` in priority order, skipping any of the three that has no artwork —
/// the caller tries each in turn and falls back to a themed placeholder if all fail.
///
/// The id returned is the one that **holds** the image, which is not always the item it belongs to
/// (see [`loxia_core::model::ImageRef`]).
pub fn art_candidates(
    track: &Track,
    album: Option<&Album>,
    artist: Option<&Artist>,
) -> Vec<(ItemId, String)> {
    [
        track.image.as_ref(),
        album.and_then(|a| a.image.as_ref()),
        artist.and_then(|a| a.image.as_ref()),
    ]
    .into_iter()
    .flatten()
    .map(|image| (image.item.clone(), image.tag.clone()))
    .collect()
}

/// Resolves candidates for whatever single `MediaItem` is being displayed. Always calls
/// [`art_candidates`] with `album: None, artist: None` for a `Track` — this widget only ever has
/// the one selected/inspected item on hand, never separately its parent album/artist objects (the
/// inspector's own `active_column`/Miller stack doesn't reliably carry them). The richer 3-tier
/// chain is fully implemented and tested above; only *reaching* it with real album/artist data is
/// left unresolved here, the same shape `09-04` left ReplayGain's normalization fallback in
/// (`docs/12-decisions.md`).
pub fn candidates_for_item(item: Option<&MediaItem>) -> Vec<(ItemId, String)> {
    match item {
        Some(MediaItem::Track(t)) => art_candidates(t, None, None),
        // The image's own item id, not the album's/artist's — for most albums Emby holds the
        // cover on a child item and only points at it (`model::ImageRef`).
        Some(MediaItem::Album(a)) => a
            .image
            .iter()
            .map(|i| (i.item.clone(), i.tag.clone()))
            .collect(),
        Some(MediaItem::Artist(a)) => a
            .image
            .iter()
            .map(|i| (i.item.clone(), i.tag.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Draws whatever art `subject` resolves to: the cached `Protocol` if one already exists for
/// `(the first candidate's id, area's own cell size)`, the placeholder otherwise — additionally
/// requesting a fetch for that first candidate, since nothing is cached for it yet. Never decodes
/// or encodes anything itself.
pub fn render(
    f: &mut Frame,
    area: Rect,
    _ctx: ArtSizeContext,
    art: &mut Art<'_>,
    subject: Option<&MediaItem>,
    theme: &Theme,
) -> Vec<Effect> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }

    let ArtRenderer::Picker(picker) = art.renderer else {
        draw_placeholder(f, area, theme);
        return Vec::new();
    };

    let Some((id, tag)) = candidates_for_item(subject).into_iter().next() else {
        draw_placeholder(f, area, theme);
        return Vec::new();
    };

    let cell = (area.width, area.height);
    if art.cache.get(&id, cell).is_none() {
        // Not encoded at this size yet — but the worker may already have decoded the pixels. This
        // is the one place encoding happens, so it costs at most once per (item, size).
        let decoded = art
            .store
            .lock()
            .ok()
            .and_then(|store| store.get(&id).cloned());
        if let Some(image) = decoded
            && let Ok(protocol) = picker.clone().new_protocol(
                image,
                Size::new(area.width, area.height),
                Resize::Fit(None),
            )
        {
            art.cache.insert(id.clone(), cell, protocol);
        }
    }

    if let Some(protocol) = art.cache.get(&id, cell) {
        f.render_widget(Image::new(protocol), centred_in(area, protocol.size()));
        return Vec::new();
    }

    draw_placeholder(f, area, theme);
    vec![Effect::Net(NetEffect::FetchImage {
        id,
        size: fetch_size_for(area.height),
        tag,
    })]
}

/// Where to actually blit an image of `size` inside `area`.
///
/// `Resize::Fit` preserves the cover's aspect ratio against the terminal's *real* cell ratio, which
/// is rarely the 2:1 [`art_rect`] assumes — so the encoded image is usually a little narrower (or
/// shorter) than the rect reserved for it. `Image` blits at the rect's top-left corner, so that
/// slack all landed on one side and the cover sat visibly off-centre in both play views, most
/// obviously when it was small enough for the slack to be a large share of the box
/// (`docs/12-decisions.md`).
///
/// Clamped to `area`: `Image` refuses to draw at all rather than clip, so a size larger than the
/// rect would silently render nothing.
fn centred_in(area: Rect, size: ratatui::layout::Size) -> Rect {
    let width = size.width.min(area.width);
    let height = size.height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// A bordered box with a centred `♪` in `Dim` — never an empty hole, which reads as a rendering
/// bug (`docs/07-ui-spec.md` §11's own wording, matching the equivalent rule for a missing image
/// elsewhere in this codebase). `pub(crate)`: `10-02`'s own Zen view draws this same placeholder
/// for its own art region — Zen has no `ArtCache` of its own to attempt real art with yet (see
/// this module's own top-level doc comment on why that's deferred), but "no art available" should
/// still look identical everywhere it appears.
pub(crate) fn draw_placeholder(f: &mut Frame, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(style::border(theme, false));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let row = inner.y + inner.height / 2;
    let line = Paragraph::new(Line::from(Span::styled(
        "♪",
        style::style(theme, Role::Dim),
    )))
    .alignment(Alignment::Center);
    f.render_widget(line, Rect::new(inner.x, row, inner.width, 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end for the store hand-off: the worker decodes into `ArtStore`, and the next render
    /// encodes it into the cache and blits it — no placeholder, and no repeat fetch
    /// (`docs/12-decisions.md`).
    /// `Resize::Fit` keeps the cover square against the terminal's real cell ratio, so the encoded
    /// image rarely fills the rect reserved for it — and `Image` blits at the top-left, which put
    /// all of that slack on one side. The cover sat visibly off-centre in both play views
    /// (`docs/12-decisions.md`).
    #[test]
    fn a_cover_smaller_than_its_box_is_centred_in_it() {
        use ratatui::layout::Size;

        let area = Rect::new(4, 2, 20, 10);
        // Narrower and shorter than the box: the slack must be split, not all trailing.
        let placed = centred_in(area, Size::new(10, 6));
        assert_eq!(placed, Rect::new(9, 4, 10, 6));

        // Exactly filling it changes nothing.
        assert_eq!(centred_in(area, Size::new(20, 10)), area);

        // Larger than the box is clamped rather than left oversized — `Image` draws nothing at all
        // rather than clipping, so an unclamped rect would render an empty hole.
        assert_eq!(centred_in(area, Size::new(40, 40)), area);

        // An odd amount of slack must not overflow the box on the far side.
        let placed = centred_in(area, Size::new(9, 5));
        assert!(placed.x >= area.x && placed.right() <= area.right());
        assert!(placed.y >= area.y && placed.bottom() <= area.bottom());
    }

    #[test]
    fn a_decoded_image_in_the_store_is_encoded_and_drawn() {
        let renderer = ArtRenderer::Picker(Picker::halfblocks());
        let mut cache = ArtCache::new();
        let store = ArtStore::default();
        let theme = Theme::default();
        let track = track_with_tag(Some("tag-1"));
        let id = track.album_id.clone().unwrap_or(track.id.clone());
        let item = MediaItem::Track(track);

        store.lock().unwrap().insert(
            id,
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                64,
                64,
                image::Rgb([200, 30, 30]),
            )),
        );

        let mut effects = Vec::new();
        let rendered = render_at(20, 10, |f| {
            let area = f.area();
            effects = render(
                f,
                area,
                ArtSizeContext::Inspector,
                &mut Art {
                    renderer: &renderer,
                    cache: &mut cache,
                    store: &store,
                },
                Some(&item),
                &theme,
            );
        });

        assert!(
            effects.is_empty(),
            "already have the pixels; must not refetch"
        );
        assert_eq!(cache.len(), 1, "the decode is encoded once and cached");
        assert!(
            !rendered.contains('\u{266a}'),
            "real art must replace the placeholder: {rendered}"
        );
    }
    use loxia_core::model::{AlbumRelation, AudioFormat, Codec};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Instant;

    fn render_at(w: u16, h: u16, f: impl FnOnce(&mut Frame)) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(f).unwrap();
        format!("{:?}", terminal.backend().buffer())
    }

    fn solid_image(color: [u8; 3]) -> DynamicImage {
        DynamicImage::from(image::RgbImage::from_pixel(8, 8, image::Rgb(color)))
    }

    /// Large enough (in pixels) that `Resize::Fit`, given `Picker::halfblocks`'s own default
    /// `FontSize`, actually needs to shrink it to the target cell area rather than leaving it at
    /// its own (tiny) native cell size — otherwise a snapshot would only ever show a single
    /// half-block in one corner, which proves nothing about how a real photo fills the pane.
    fn large_solid_image(color: [u8; 3]) -> DynamicImage {
        DynamicImage::from(image::RgbImage::from_pixel(400, 400, image::Rgb(color)))
    }

    fn track_with_tag(tag: Option<&str>) -> Track {
        Track {
            id: ItemId::from("t1"),
            name: "Song".to_string(),
            album_id: None,
            album_name: "Album".to_string(),
            album_artist_names: Vec::new(),
            artist_ids: Vec::new(),
            artist_names: Vec::new(),
            track_number: None,
            disc_number: None,
            year: None,
            duration: std::time::Duration::ZERO,
            genres: Vec::new(),
            is_favorite: false,
            play_count: 0,
            format: AudioFormat {
                codec: Codec::Flac,
                sample_rate_hz: 44_100,
                bit_depth: Some(16),
                channels: 2,
                bitrate_bps: None,
            },
            replay_gain: None,
            image: tag.map(|t| loxia_core::model::ImageRef {
                item: ItemId::from("t1"),
                tag: t.to_string(),
            }),
            media_source_id: None,
            lyric_stream: None,
            date_created: None,
            playlist_entry_id: None,
        }
    }

    fn album_with_tag(tag: Option<&str>) -> Album {
        Album {
            id: ItemId::from("al1"),
            name: "Album".to_string(),
            sort_name: "Album".to_string(),
            album_artist_names: Vec::new(),
            album_artist_ids: Vec::new(),
            year: None,
            track_count: 0,
            total_duration: std::time::Duration::ZERO,
            genres: Vec::new(),
            is_favorite: false,
            image: tag.map(|t| loxia_core::model::ImageRef {
                item: ItemId::from("al1"),
                tag: t.to_string(),
            }),
            relation: AlbumRelation::Primary,
        }
    }

    fn artist_with_tag(tag: Option<&str>) -> Artist {
        Artist {
            id: ItemId::from("ar1"),
            name: "Artist".to_string(),
            sort_name: "Artist".to_string(),
            album_count: 0,
            track_count: 0,
            genres: Vec::new(),
            is_favorite: false,
            image: tag.map(|t| loxia_core::model::ImageRef {
                item: ItemId::from("ar1"),
                tag: t.to_string(),
            }),
            overview: None,
        }
    }

    /// The query thread cannot be cancelled — it parks on a blocking `io::stdin().read()` — so
    /// abandoning it leaves it racing the real input reader for keystrokes. Waiting longer than
    /// `ratatui-image`'s own 2 s stdin budget is what keeps that from happening to any terminal
    /// that merely answers slowly (`docs/12-decisions.md`).
    #[test]
    fn detection_outwaits_the_querys_own_stdin_budget() {
        const RATATUI_IMAGE_STDIN_TIMEOUT: Duration = Duration::from_millis(2000);
        assert!(
            DETECT_TIMEOUT > RATATUI_IMAGE_STDIN_TIMEOUT,
            "abandoning a query thread that is still reading stdin breaks the keyboard"
        );
    }

    /// A terminal that answers promptly must not be made to wait out the timeout.
    #[test]
    fn a_prompt_answer_returns_immediately() {
        let start = Instant::now();
        let _ = detect_renderer_with(ArtProtocol::Auto, Picker::halfblocks);
        assert!(start.elapsed() < Duration::from_millis(250));
    }

    #[test]
    fn protocol_detection_falls_back_on_timeout() {
        let start = Instant::now();
        let renderer = detect_renderer_with(ArtProtocol::Auto, || {
            thread::sleep(Duration::from_secs(999));
            Picker::halfblocks()
        });
        assert!(
            start.elapsed() < DETECT_TIMEOUT * 2,
            "must not wait anywhere near the slow query's own sleep"
        );
        match renderer {
            ArtRenderer::Picker(p) => assert_eq!(p.protocol_type(), ProtocolType::Halfblocks),
            ArtRenderer::Off => panic!("Auto must never resolve to Off"),
        }
    }

    #[test]
    fn forced_protocol_skips_detection() {
        let renderer =
            detect_renderer_with(ArtProtocol::Kitty, || panic!("must not query the terminal"));
        match renderer {
            ArtRenderer::Picker(p) => assert_eq!(p.protocol_type(), ProtocolType::Kitty),
            ArtRenderer::Off => panic!("Kitty must not resolve to Off"),
        }
    }

    #[test]
    fn forced_off_never_queries() {
        let renderer = detect_renderer_with(ArtProtocol::Off, || panic!("Off must not query"));
        assert!(matches!(renderer, ArtRenderer::Off));
    }

    #[test]
    fn decoded_cache_evicts_at_sixteen() {
        let picker = Picker::halfblocks();
        let mut cache = ArtCache::new();
        let _store = ArtStore::default();
        for i in 0..17u32 {
            let id = ItemId::from(format!("item-{i}"));
            let protocol = encode(&picker, solid_image([1, 2, 3]), (4, 2)).unwrap();
            cache.insert(id, (4, 2), protocol);
        }
        assert_eq!(cache.len(), 16);
        assert!(
            cache.get(&ItemId::from("item-0"), (4, 2)).is_none(),
            "the oldest entry must be evicted"
        );
        assert!(cache.get(&ItemId::from("item-16"), (4, 2)).is_some());
    }

    #[test]
    fn cache_key_includes_cell_size() {
        let picker = Picker::halfblocks();
        let mut cache = ArtCache::new();
        let _store = ArtStore::default();
        let id = ItemId::from("same-item");
        cache.insert(
            id.clone(),
            (4, 2),
            encode(&picker, solid_image([1, 1, 1]), (4, 2)).unwrap(),
        );
        cache.insert(
            id.clone(),
            (8, 4),
            encode(&picker, solid_image([1, 1, 1]), (8, 4)).unwrap(),
        );
        assert_eq!(cache.len(), 2, "the same image at two sizes is two entries");
    }

    #[test]
    fn fallback_chain_order() {
        let candidates = art_candidates(
            &track_with_tag(Some("track-tag")),
            Some(&album_with_tag(Some("album-tag"))),
            Some(&artist_with_tag(Some("artist-tag"))),
        );
        assert_eq!(
            candidates,
            vec![
                (ItemId::from("t1"), "track-tag".to_string()),
                (ItemId::from("al1"), "album-tag".to_string()),
                (ItemId::from("ar1"), "artist-tag".to_string()),
            ]
        );
    }

    #[test]
    fn fallback_chain_skips_missing_tags() {
        let candidates = art_candidates(
            &track_with_tag(None),
            Some(&album_with_tag(Some("album-tag"))),
            Some(&artist_with_tag(None)),
        );
        assert_eq!(
            candidates,
            vec![(ItemId::from("al1"), "album-tag".to_string())]
        );
    }

    #[test]
    fn placeholder_drawn_when_no_art_anywhere() {
        let renderer = ArtRenderer::Picker(Picker::halfblocks());
        let mut cache = ArtCache::new();
        let store = ArtStore::default();
        let theme = Theme::default();
        let item = MediaItem::Track(track_with_tag(None));
        let mut effects = Vec::new();
        let rendered = render_at(20, 10, |f| {
            let area = f.area();
            effects = render(
                f,
                area,
                ArtSizeContext::Inspector,
                &mut Art {
                    renderer: &renderer,
                    cache: &mut cache,
                    store: &store,
                },
                Some(&item),
                &theme,
            );
        });
        assert!(rendered.contains('♪'));
        assert!(
            effects.is_empty(),
            "no candidates at all means no fetch either"
        );
    }

    #[test]
    fn decode_happens_in_worker_not_render() {
        let renderer = ArtRenderer::Picker(Picker::halfblocks());
        let mut cache = ArtCache::new();
        let store = ArtStore::default();
        let theme = Theme::default();
        let track = track_with_tag(Some("tag-1"));
        let item = MediaItem::Track(track.clone());
        let mut effects = Vec::new();
        let rendered = render_at(20, 10, |f| {
            let area = f.area();
            effects = render(
                f,
                area,
                ArtSizeContext::Inspector,
                &mut Art {
                    renderer: &renderer,
                    cache: &mut cache,
                    store: &store,
                },
                Some(&item),
                &theme,
            );
        });
        assert!(
            rendered.contains('♪'),
            "nothing cached yet: still the placeholder"
        );
        assert_eq!(
            effects,
            vec![Effect::Net(NetEffect::FetchImage {
                id: track.id,
                // A 10-row area upscales a 64px thumb, so the full-size asset is requested.
                size: ImageSize::Large,
                tag: "tag-1".to_string(),
            })]
        );
    }

    /// The fetch size follows the area actually being filled, not the calling context — a 64px
    /// thumb stretched over a large pane is what made covers look blocky (`docs/12-decisions.md`).
    #[test]
    fn fetch_size_follows_the_rendered_area() {
        assert_eq!(fetch_size_for(4), ImageSize::Thumb);
        assert_eq!(fetch_size_for(THUMB_MAX_ROWS), ImageSize::Thumb);
        assert_eq!(fetch_size_for(THUMB_MAX_ROWS + 1), ImageSize::Large);
        assert_eq!(fetch_size_for(24), ImageSize::Large);
    }

    /// Art scales to whatever the pane offers rather than a fixed per-context cap, which used to
    /// leave covers postage-stamp-sized on a large terminal (`docs/12-decisions.md`).
    #[test]
    fn art_scales_to_the_available_pane() {
        // Height-bound: 30 rows would need 60 columns, but only 40 are available -> 20 rows.
        let wide = art_rect(ArtSizeContext::Zen, Rect::new(0, 0, 40, 30));
        assert_eq!((wide.width, wide.height), (40, 20));

        // Width-bound the other way: plenty of columns, only 8 rows to use.
        let short = art_rect(ArtSizeContext::Inspector, Rect::new(0, 0, 80, 8));
        assert_eq!((short.width, short.height), (16, 8));

        // A bigger pane genuinely yields bigger art — the whole point.
        let small = art_rect(ArtSizeContext::Inspector, Rect::new(0, 0, 24, 12));
        let large = art_rect(ArtSizeContext::Inspector, Rect::new(0, 0, 60, 40));
        assert!(large.height > small.height);
    }

    #[test]
    fn stale_image_deleted_on_track_change() {
        let picker = Picker::halfblocks();
        let mut cache = ArtCache::new();
        let _store = ArtStore::default();
        let id = ItemId::from("track-a");
        cache.insert(
            id.clone(),
            (4, 2),
            encode(&picker, solid_image([9, 9, 9]), (4, 2)).unwrap(),
        );
        assert!(cache.get(&id, (4, 2)).is_some());

        cache.evict_item(&id);

        assert!(cache.get(&id, (4, 2)).is_none());
    }

    #[test]
    fn art_snapshot_halfblocks() {
        let picker = Picker::halfblocks();
        let mut cache = ArtCache::new();
        let store = ArtStore::default();
        let id = ItemId::from("t1");
        let protocol = encode(&picker, large_solid_image([200, 50, 50]), (10, 5)).unwrap();
        cache.insert(id.clone(), (10, 5), protocol);
        let renderer = ArtRenderer::Picker(picker);
        let theme = Theme::default();
        let mut track = track_with_tag(Some("tag"));
        track.id = id;
        let item = MediaItem::Track(track);

        let rendered = render_at(10, 5, |f| {
            let area = f.area();
            render(
                f,
                area,
                ArtSizeContext::Inspector,
                &mut Art {
                    renderer: &renderer,
                    cache: &mut cache,
                    store: &store,
                },
                Some(&item),
                &theme,
            );
        });
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn art_snapshot_placeholder() {
        let renderer = ArtRenderer::Picker(Picker::halfblocks());
        let mut cache = ArtCache::new();
        let store = ArtStore::default();
        let theme = Theme::default();
        let rendered = render_at(10, 5, |f| {
            let area = f.area();
            render(
                f,
                area,
                ArtSizeContext::Inspector,
                &mut Art {
                    renderer: &renderer,
                    cache: &mut cache,
                    store: &store,
                },
                None,
                &theme,
            );
        });
        insta::assert_snapshot!(rendered);
    }
}
