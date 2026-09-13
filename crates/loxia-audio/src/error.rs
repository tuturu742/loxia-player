//! AudioError and platform install hints.
//!
//! Every message is a fixed, generic sentence, never interpolating a field's actual content —
//! the same convention `loxia-emby::error::EmbyError` already establishes, so an arbitrary
//! `source`/`reason` string can never break the "single sentence, ends with a period" contract.
//! Detail lives in the struct fields for logging/structured access, not in `Display`.

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AudioError {
    #[error("could not find the mpv library.")]
    LibraryNotFound { hint: String },
    #[error("the audio engine failed to initialise.")]
    Init(String),
    // Named `detail`, not `source` (the task's own literal spec) — `thiserror` treats a field
    // literally named `source` as the implementation of `Error::source()`, which requires it to
    // implement `std::error::Error`; a plain diagnostic `String` does not, and does not need to.
    #[error("an mpv command failed.")]
    Command { cmd: &'static str, detail: String },
    #[error("could not load the track.")]
    Load {
        url_redacted: String,
        reason: String,
    },
    #[error("the selected audio device is unavailable.")]
    DeviceUnavailable { id: String },
    #[error("the audio engine has shut down.")]
    Shutdown,
}

/// The per-OS install instruction for `AudioError::LibraryNotFound`'s `hint` — turning a
/// confusing "library not found" crash at startup into a one-line fix.
///
/// `09-01`: switched from three compile-time OS-conditional branches to a runtime match on
/// `std::env::consts::OS` — `docs/README.md` rule 5 confines that particular conditional-
/// compilation attribute to `loxia-audio::device` and `loxia-core::paths` (so this crate's own
/// `cfg_blocks_confined_to_device_modules` grep test can enforce it project-wide); this function
/// needs a per-OS *string*, not per-OS *code*, and `std::env::consts::OS` gives the identical
/// result — it's a `const` reflecting the actual compiled target — without the attribute
/// (`docs/12-decisions.md`).
pub fn library_not_found_hint() -> String {
    match std::env::consts::OS {
        "macos" => "install it with Homebrew: `brew install mpv`.".to_string(),
        "linux" => "install mpv from your distribution's repositories (it provides libmpv) — \
                     e.g. `apt install mpv`, `dnf install mpv-libs`, or `pacman -S mpv`."
            .to_string(),
        "windows" => {
            "the loxia-player Windows installer bundles `mpv-1.dll`; reinstall loxia-player if it is missing."
                .to_string()
        }
        _ => "install mpv (https://mpv.io) for your platform.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_error_display_is_single_sentence() {
        let cases: Vec<AudioError> = vec![
            AudioError::LibraryNotFound {
                hint: library_not_found_hint(),
            },
            AudioError::Init("boom".to_string()),
            AudioError::Command {
                cmd: "loadfile",
                detail: "timed out".to_string(),
            },
            AudioError::Load {
                url_redacted: "http://host/stream".to_string(),
                reason: "404".to_string(),
            },
            AudioError::DeviceUnavailable {
                id: "alsa/hw:0,0".to_string(),
            },
            AudioError::Shutdown,
        ];
        for e in cases {
            let s = e.to_string();
            assert!(
                !s.contains('\n'),
                "variant {e:?} display contains a newline: {s:?}"
            );
            assert!(
                s.ends_with('.'),
                "variant {e:?} display does not end with '.': {s:?}"
            );
        }
    }

    #[test]
    fn library_not_found_hint_is_platform_specific() {
        let hint = library_not_found_hint();
        match std::env::consts::OS {
            "macos" => assert!(hint.contains("brew")),
            "linux" => assert!(hint.contains("distribution")),
            "windows" => assert!(hint.contains("mpv-1.dll")),
            _ => assert!(hint.contains("mpv.io")),
        }
    }
}
