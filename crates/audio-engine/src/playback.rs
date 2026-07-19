use std::array;
use std::sync::Arc;

use crate::CHANNELS;

pub const MAX_ACTIVE_VOICES: usize = 16;
const MAX_RETIRED_BUFFERS: usize = MAX_ACTIVE_VOICES + 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayPolicy {
    Overlap,
    Toggle,
    Stop,
    Restart,
}

struct Voice {
    id: PlaybackId,
    samples: Arc<[f32]>,
    position: usize,
    gain: f32,
    paused: bool,
    generation: u64,
}

pub(crate) struct RetiredBuffers {
    items: [Option<Arc<[f32]>>; MAX_RETIRED_BUFFERS],
}

impl RetiredBuffers {
    fn new() -> Self {
        Self {
            items: array::from_fn(|_| None),
        }
    }

    fn push(&mut self, buffer: Arc<[f32]>) {
        if let Some(slot) = self.items.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(buffer);
        }
    }

    pub(crate) fn drain(&mut self) -> impl Iterator<Item = Arc<[f32]>> + '_ {
        self.items.iter_mut().filter_map(Option::take)
    }
}

pub(crate) struct PolyphonicPlayer {
    voices: [Option<Voice>; MAX_ACTIVE_VOICES],
    generation: u64,
    last_id: Option<PlaybackId>,
    last_total_frames: u64,
    last_position_frames: u64,
    last_paused: bool,
}

impl PolyphonicPlayer {
    pub(crate) fn new() -> Self {
        Self {
            voices: array::from_fn(|_| None),
            generation: 0,
            last_id: None,
            last_total_frames: 0,
            last_position_frames: 0,
            last_paused: false,
        }
    }

    pub(crate) fn trigger(
        &mut self,
        id: PlaybackId,
        samples: Arc<[f32]>,
        gain: f32,
        policy: ReplayPolicy,
    ) -> RetiredBuffers {
        let mut retired = RetiredBuffers::new();
        let matching = self
            .voices
            .iter()
            .any(|voice| voice.as_ref().is_some_and(|voice| voice.id == id));
        match policy {
            ReplayPolicy::Overlap => self.start_voice(id, samples, gain, &mut retired),
            ReplayPolicy::Toggle if matching => {
                let should_pause = self.voices.iter().any(|voice| {
                    voice
                        .as_ref()
                        .is_some_and(|voice| voice.id == id && !voice.paused)
                });
                for voice in self.voices.iter_mut().flatten() {
                    if voice.id == id {
                        voice.paused = should_pause;
                    }
                }
                self.last_id = Some(id);
                self.last_paused = should_pause;
                retired.push(samples);
            }
            ReplayPolicy::Stop if matching => {
                self.retire_matching(id, &mut retired);
                self.clear_last(id);
                retired.push(samples);
            }
            ReplayPolicy::Restart if matching => {
                self.retire_matching(id, &mut retired);
                self.start_voice(id, samples, gain, &mut retired);
            }
            ReplayPolicy::Toggle | ReplayPolicy::Stop | ReplayPolicy::Restart => {
                self.start_voice(id, samples, gain, &mut retired);
            }
        }
        retired
    }

    pub(crate) fn stop_all(&mut self) -> RetiredBuffers {
        let mut retired = RetiredBuffers::new();
        for slot in &mut self.voices {
            if let Some(voice) = slot.take() {
                retired.push(voice.samples);
            }
        }
        self.last_id = None;
        self.last_total_frames = 0;
        self.last_position_frames = 0;
        self.last_paused = false;
        retired
    }

    pub(crate) fn render(&mut self, output: &mut [f32]) -> RetiredBuffers {
        output.fill(0.0);
        let mut retired = RetiredBuffers::new();
        for slot in &mut self.voices {
            let Some(voice) = slot.as_mut() else {
                continue;
            };
            if voice.paused {
                continue;
            }
            let remaining = voice.samples.len().saturating_sub(voice.position);
            let copied = remaining.min(output.len());
            for (destination, source) in output[..copied]
                .iter_mut()
                .zip(&voice.samples[voice.position..voice.position + copied])
            {
                *destination += *source * voice.gain;
            }
            voice.position += copied;
            if self.last_id == Some(voice.id) {
                self.last_position_frames = (voice.position / CHANNELS as usize) as u64;
                self.last_paused = false;
            }
            if voice.position >= voice.samples.len() {
                let completed = slot.take().expect("completed voice exists");
                retired.push(completed.samples);
            }
        }
        retired
    }

    pub(crate) fn played_frames(&self) -> u64 {
        self.last_position_frames
    }

    pub(crate) fn total_frames(&self) -> u64 {
        self.last_total_frames
    }

    pub(crate) fn paused(&self) -> bool {
        self.last_paused
    }

    pub(crate) fn active_voice_count(&self) -> usize {
        self.voices.iter().flatten().count()
    }

    fn start_voice(
        &mut self,
        id: PlaybackId,
        samples: Arc<[f32]>,
        gain: f32,
        retired: &mut RetiredBuffers,
    ) {
        self.generation = self.generation.wrapping_add(1);
        let total_frames = (samples.len() / CHANNELS as usize) as u64;
        let slot_index = self
            .voices
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .filter_map(|(index, voice)| {
                        voice.as_ref().map(|voice| (index, voice.generation))
                    })
                    .min_by_key(|(_, generation)| *generation)
                    .map_or(0, |(index, _)| index)
            });
        if let Some(replaced) = self.voices[slot_index].take() {
            retired.push(replaced.samples);
        }
        self.voices[slot_index] = Some(Voice {
            id,
            samples,
            position: 0,
            gain: gain.clamp(0.0, 2.0),
            paused: false,
            generation: self.generation,
        });
        self.last_id = Some(id);
        self.last_total_frames = total_frames;
        self.last_position_frames = 0;
        self.last_paused = false;
    }

    fn retire_matching(&mut self, id: PlaybackId, retired: &mut RetiredBuffers) {
        for slot in &mut self.voices {
            if slot.as_ref().is_some_and(|voice| voice.id == id) {
                let voice = slot.take().expect("matching voice exists");
                retired.push(voice.samples);
            }
        }
    }

    fn clear_last(&mut self, id: PlaybackId) {
        if self.last_id == Some(id) {
            self.last_id = None;
            self.last_total_frames = 0;
            self.last_position_frames = 0;
            self.last_paused = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound(value: f32, frames: usize) -> Arc<[f32]> {
        Arc::from(vec![value; frames * CHANNELS as usize].into_boxed_slice())
    }

    #[test]
    fn overlap_mixes_voices_and_applies_per_sound_gain() {
        let mut player = PolyphonicPlayer::new();
        player.trigger(PlaybackId(1), sound(0.2, 4), 0.5, ReplayPolicy::Overlap);
        player.trigger(PlaybackId(1), sound(0.3, 4), 1.0, ReplayPolicy::Overlap);
        let mut output = [0.0; 4];
        player.render(&mut output);
        assert_eq!(output, [0.4; 4]);
        assert_eq!(player.active_voice_count(), 2);
    }

    #[test]
    fn toggle_pauses_and_resumes_without_losing_position() {
        let mut player = PolyphonicPlayer::new();
        player.trigger(PlaybackId(1), sound(0.5, 8), 1.0, ReplayPolicy::Toggle);
        let mut output = [0.0; 4];
        player.render(&mut output);
        player.trigger(PlaybackId(1), sound(0.5, 8), 1.0, ReplayPolicy::Toggle);
        assert!(player.paused());
        player.render(&mut output);
        assert_eq!(output, [0.0; 4]);
        assert_eq!(player.played_frames(), 2);
        player.trigger(PlaybackId(1), sound(0.5, 8), 1.0, ReplayPolicy::Toggle);
        assert!(!player.paused());
        player.render(&mut output);
        assert_eq!(player.played_frames(), 4);
    }

    #[test]
    fn stop_and_restart_have_deterministic_retrigger_behavior() {
        let mut player = PolyphonicPlayer::new();
        player.trigger(PlaybackId(1), sound(0.4, 8), 1.0, ReplayPolicy::Stop);
        player.trigger(PlaybackId(1), sound(0.4, 8), 1.0, ReplayPolicy::Stop);
        assert_eq!(player.active_voice_count(), 0);

        player.trigger(PlaybackId(2), sound(0.6, 8), 1.0, ReplayPolicy::Restart);
        let mut output = [0.0; 4];
        player.render(&mut output);
        assert_eq!(player.played_frames(), 2);
        player.trigger(PlaybackId(2), sound(0.6, 8), 1.0, ReplayPolicy::Restart);
        assert_eq!(player.played_frames(), 0);
        assert_eq!(player.active_voice_count(), 1);
    }

    #[test]
    fn voice_capacity_retires_the_oldest_voice() {
        let mut player = PolyphonicPlayer::new();
        for id in 0..=MAX_ACTIVE_VOICES {
            player.trigger(
                PlaybackId(id as u64),
                sound(0.01, 8),
                1.0,
                ReplayPolicy::Overlap,
            );
        }
        assert_eq!(player.active_voice_count(), MAX_ACTIVE_VOICES);
        assert!(
            !player
                .voices
                .iter()
                .flatten()
                .any(|voice| voice.id == PlaybackId(0))
        );
    }
}
