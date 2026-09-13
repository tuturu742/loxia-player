//! Effect -> worker routing.

use loxia_core::effect::Effect;
use tokio::sync::mpsc::UnboundedSender;

use crate::workers::Workers;

/// Fans `effect` out to every worker's channel — each worker's own receive loop decides which
/// variants it cares about (`docs/01-architecture.md` §4, `crate::workers`'s module doc). A
/// closed channel means a dead worker; logged at `error` and otherwise ignored, since a dead
/// worker must never take down the UI.
///
/// `Effect::Sys(Exit)` is never routed here — `runtime::run` intercepts it before calling
/// `dispatch`, since ending the loop is the runtime's own job, not a worker's.
pub fn dispatch(effect: &Effect, workers: &Workers) {
    send(&workers.network, effect, "network");
    send(&workers.audio, effect, "audio");
    send(&workers.cache, effect, "cache");
    send(&workers.mpris, effect, "mpris");
    send(&workers.notify, effect, "notify");
}

fn send(tx: &UnboundedSender<Effect>, effect: &Effect, worker: &'static str) {
    if tx.send(effect.clone()).is_err() {
        tracing::error!(worker, "effect dropped: worker channel is closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loxia_core::effect::{AudioEffect, SysEffect};

    #[test]
    fn closed_worker_channel_does_not_panic() {
        let (workers, receivers) = Workers::for_test();
        // Drop one worker's receiver, simulating a dead worker, before anything is sent.
        let [network_rx, audio_rx, cache_rx, mpris_rx, notify_rx] = receivers;
        drop(audio_rx);
        drop((network_rx, cache_rx, mpris_rx, notify_rx));

        dispatch(&Effect::Audio(AudioEffect::PlayPause), &workers);
        dispatch(&Effect::Sys(SysEffect::Exit), &workers); // still just fans out; harmless here
    }

    #[test]
    fn effect_reaches_every_worker() {
        let (workers, mut receivers) = Workers::for_test();
        dispatch(&Effect::Audio(AudioEffect::PlayPause), &workers);
        for rx in &mut receivers {
            assert_eq!(
                rx.try_recv(),
                Ok(Effect::Audio(AudioEffect::PlayPause)),
                "every worker channel should receive a clone of the effect"
            );
        }
    }
}
