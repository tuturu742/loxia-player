//! `AudioDevice` grouping and labelling (`09-01`, `docs/05-audio-engine.md` §4). Enumeration itself
//! (mpv's `audio-device-list` property) and hot-swap (`AudioCommand::SetDevice`) live in
//! `mpv/handle.rs`, which already owns every other mpv property/command translation; this module is
//! the OS-agnostic part they feed into — grouping and labelling devices for the device picker.
//!
//! Nothing here is platform-specific any more: the per-OS `linux`/`macos`/`windows` children existed
//! solely for bit-perfect capability detection, and went with it (`docs/12-decisions.md`).

/// Grouping and labelling are pure functions of `AudioDevice`'s own fields, so they live in
/// `loxia-core` (needed there by `loxia-tui`'s device-picker modal, which cannot depend on this
/// crate) and are re-exported here for this module's own existing callers/tests.
pub use loxia_core::model::{device_label as label, group_by_driver};

#[cfg(test)]
mod tests {

    /// `docs/README.md` rule 5: `#[cfg(target_os = ...)]` is confined to `loxia-audio::device` and
    /// `loxia-core::paths`. A grep test over this crate's own source tree, not a doc-comment
    /// promise. Since bit-perfect's removal took the per-OS children with it, this crate now has no
    /// platform conditionals at all — the test stands as a guard against reintroducing one loosely.
    #[test]
    fn cfg_blocks_confined_to_device_modules() {
        let src_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut offenders = Vec::new();
        walk(src_dir, &mut offenders);
        assert!(
            offenders.is_empty(),
            "found #[cfg(target_os ...)] outside device/: {offenders:?}"
        );
    }

    fn walk(dir: &std::path::Path, offenders: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, offenders);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            if path.components().any(|c| c.as_os_str() == "device") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            if text.contains("target_os") {
                offenders.push(path);
            }
        }
    }
}
