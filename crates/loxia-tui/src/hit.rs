//! `HitMap`/`HitTarget` — the mouse hit-testing registry (`docs/04-state-and-input.md` §8).
//!
//! Widgets register their regions during `draw`; the runtime (task `10-04`) resolves mouse events
//! against the *previous* frame's map. Landed ahead of its own task number (`04-04`) because
//! `04-02`'s `draw()` signature already needs a real `HitMap` type to compile — see
//! `docs/12-decisions.md`.

use loxia_core::keymap::ActionId;
use loxia_core::model::QueueEntryId;
use loxia_core::state::nav::Tab;
use loxia_core::state::search::SearchSection;
use loxia_core::state::settings::SettingsSection;
use ratatui::layout::Rect;

/// The physical transport buttons in the player bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportButton {
    Prev,
    PlayPause,
    Stop,
    Next,
    Shuffle,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    SidebarTab(Tab),
    ColumnItem {
        column: usize,
        index: usize,
    },
    QueueEntry(QueueEntryId),
    SeekBar,
    VolumeBar,
    Transport(TransportButton),
    ModalField(usize),
    EqBand(usize),
    InspectorAction(ActionId),
    /// A row in one of the shared three-section list's sections (`widgets::sectioned_list`,
    /// `07-01`/`07-02`: Search and Favourites both use it) — a separate variant from `ColumnItem`
    /// since neither tab has a Miller column/depth of its own to reuse that shape.
    SectionedListItem {
        section: SearchSection,
        index: usize,
    },
    /// The Search tab's query line itself, so a click on it can focus it (`07-01`).
    SearchQuery,
    /// `11-01`: a row in the Settings tab's own left-pane section list.
    SettingsSection(SettingsSection),
    /// `11-01`: a row in the Settings tab's own right-pane control list, by index within the
    /// focused section's `reducer::settings::rows_for_section` (the same "index into a fresh,
    /// recomputed list" shape `SectionedListItem` already uses).
    SettingsRow(usize),
    /// The "Quit" row rendered in the sidebar beneath the tabs — a click quits the app.
    QuitButton,
    /// The `[?]` button in the header, immediately left of the clock — opens the help overlay,
    /// the mouse equivalent of `F1`/`?`.
    HelpButton,
    /// The whole of the Now Playing tab's queue/history pane, registered *before* the individual
    /// `QueueEntry` rows so those still win a click (`hit` takes the last match). It exists so the
    /// scroll wheel has something to land on across the entire pane — including the History
    /// sub-view and the blank space below a short queue, neither of which registers rows.
    NowPlayingList,
}

/// Reused across frames rather than reallocated (`clear` truncates, it doesn't drop) — this is
/// touched every frame and allocation churn here is measurable.
#[derive(Debug, Clone, Default)]
pub struct HitMap {
    regions: Vec<(Rect, HitTarget)>,
}

impl HitMap {
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// A zero-width or zero-height `area` is silently dropped — a widget that isn't visible must
    /// not be clickable.
    pub fn push(&mut self, area: Rect, target: HitTarget) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.regions.push((area, target));
    }

    /// The **last** matching region: widgets draw back to front, so a modal registered after the
    /// column beneath it must win. Both the first and last cell of a region count as inside it
    /// (`Rect::contains`'s own inclusive-on-all-sides definition).
    pub fn hit(&self, col: u16, row: u16) -> Option<&HitTarget> {
        let point = ratatui::layout::Position { x: col, y: row };
        self.regions
            .iter()
            .rev()
            .find(|(area, _)| area.contains(point))
            .map(|(_, target)| target)
    }

    /// As [`Self::hit`], but also returns the matched region's own `Rect` — `10-04` needs this to
    /// compute a click's *relative* position within the target (`SeekBar`'s `x_rel` fraction,
    /// `EqBand`'s gain-from-y), which the target enum alone can't carry. `HitTarget` is `Copy`, so
    /// this returns an owned pair rather than a second borrow of `self`.
    pub fn hit_rect(&self, col: u16, row: u16) -> Option<(Rect, HitTarget)> {
        let point = ratatui::layout::Position { x: col, y: row };
        self.regions
            .iter()
            .rev()
            .find(|(area, _)| area.contains(point))
            .map(|&(area, target)| (area, target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_returns_last_registered_overlapping_region() {
        let mut map = HitMap::default();
        map.push(Rect::new(0, 0, 10, 10), HitTarget::SeekBar);
        map.push(Rect::new(0, 0, 10, 10), HitTarget::VolumeBar);
        assert_eq!(map.hit(5, 5), Some(&HitTarget::VolumeBar));
    }

    #[test]
    fn hit_outside_all_regions_is_none() {
        let mut map = HitMap::default();
        map.push(Rect::new(0, 0, 10, 10), HitTarget::SeekBar);
        assert_eq!(map.hit(20, 20), None);
    }

    #[test]
    fn zero_sized_regions_are_not_registered() {
        let mut map = HitMap::default();
        map.push(Rect::new(0, 0, 0, 10), HitTarget::SeekBar);
        map.push(Rect::new(0, 0, 10, 0), HitTarget::VolumeBar);
        assert_eq!(map.hit(0, 0), None);
    }

    #[test]
    fn clear_empties_without_reallocating() {
        let mut map = HitMap::default();
        for i in 0..100u16 {
            map.push(Rect::new(i, 0, 1, 1), HitTarget::EqBand(i as usize));
        }
        let cap_before = map.regions.capacity();
        map.clear();
        assert_eq!(map.regions.len(), 0);
        assert_eq!(map.regions.capacity(), cap_before);
    }

    #[test]
    fn hit_is_inclusive_of_edges() {
        let mut map = HitMap::default();
        map.push(Rect::new(5, 5, 3, 3), HitTarget::SeekBar); // cells (5,5)..=(7,7)
        assert_eq!(map.hit(5, 5), Some(&HitTarget::SeekBar));
        assert_eq!(map.hit(7, 7), Some(&HitTarget::SeekBar));
        assert_eq!(map.hit(8, 8), None);
    }

    #[test]
    fn hit_map_survives_ten_thousand_pushes() {
        let mut map = HitMap::default();
        for i in 0..10_000u16 {
            map.push(
                Rect::new(i % 200, i / 200, 1, 1),
                HitTarget::ColumnItem {
                    column: 0,
                    index: i as usize,
                },
            );
        }
        assert_eq!(
            map.hit(50, 25),
            Some(&HitTarget::ColumnItem {
                column: 0,
                index: 25 * 200 + 50,
            })
        );
    }
}
