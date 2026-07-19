use crate::CommandReceiver;

const DEFAULT_GAIN_RAMP_FRAMES: u32 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MixBus {
    Microphone,
    Soundboard,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MixerCommand {
    SetGain { bus: MixBus, gain: f32 },
    SetMuted { bus: MixBus, muted: bool },
    ResetMeters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MixStats {
    pub peak: f32,
    pub clipped_samples: u64,
    pub processed_frames: u64,
}

#[derive(Clone, Copy, Debug)]
struct GainState {
    current: f32,
    target: f32,
    step: f32,
    frames_remaining: u32,
    muted: bool,
}

impl GainState {
    const fn unity() -> Self {
        Self {
            current: 1.0,
            target: 1.0,
            step: 0.0,
            frames_remaining: 0,
            muted: false,
        }
    }

    fn set_target(&mut self, target: f32) {
        self.target = target.clamp(0.0, 4.0);
        self.frames_remaining = DEFAULT_GAIN_RAMP_FRAMES;
        self.step = (self.target - self.current) / self.frames_remaining as f32;
    }

    fn next_frame(&mut self) -> f32 {
        if self.frames_remaining > 0 {
            self.current += self.step;
            self.frames_remaining -= 1;
            if self.frames_remaining == 0 {
                self.current = self.target;
            }
        }
        if self.muted { 0.0 } else { self.current }
    }
}

pub struct RealtimeMixer {
    channels: usize,
    commands: CommandReceiver<MixerCommand>,
    microphone: GainState,
    soundboard: GainState,
    stats: MixStats,
}

impl RealtimeMixer {
    pub fn new(channels: usize, commands: CommandReceiver<MixerCommand>) -> Self {
        assert!(channels > 0, "mixer channel count must be non-zero");
        Self {
            channels,
            commands,
            microphone: GainState::unity(),
            soundboard: GainState::unity(),
            stats: MixStats::default(),
        }
    }

    pub const fn stats(&self) -> MixStats {
        self.stats
    }

    pub fn process(&mut self, microphone: &[f32], soundboard: &[f32], output: &mut [f32]) {
        self.apply_commands();
        assert_eq!(
            output.len() % self.channels,
            0,
            "output must contain complete frames"
        );

        for (frame_index, output_frame) in output.chunks_exact_mut(self.channels).enumerate() {
            let microphone_gain = self.microphone.next_frame();
            let soundboard_gain = self.soundboard.next_frame();

            for (channel, output_sample) in output_frame.iter_mut().enumerate() {
                let sample_index = frame_index * self.channels + channel;
                let mixed = microphone.get(sample_index).copied().unwrap_or_default()
                    * microphone_gain
                    + soundboard.get(sample_index).copied().unwrap_or_default() * soundboard_gain;
                if mixed.abs() > 1.0 {
                    self.stats.clipped_samples += 1;
                }
                *output_sample = mixed.clamp(-1.0, 1.0);
                self.stats.peak = self.stats.peak.max(output_sample.abs());
            }
            self.stats.processed_frames += 1;
        }
    }

    fn apply_commands(&mut self) {
        while let Some(command) = self.commands.try_receive() {
            match command {
                MixerCommand::SetGain { bus, gain } => self.bus_mut(bus).set_target(gain),
                MixerCommand::SetMuted { bus, muted } => self.bus_mut(bus).muted = muted,
                MixerCommand::ResetMeters => self.stats = MixStats::default(),
            }
        }
    }

    fn bus_mut(&mut self, bus: MixBus) -> &mut GainState {
        match bus {
            MixBus::Microphone => &mut self.microphone,
            MixBus::Soundboard => &mut self.soundboard,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{MixBus, MixerCommand, RealtimeMixer, bounded_command_queue};

    #[test]
    fn mixes_two_sources_without_changing_in_range_samples() {
        let (_sender, receiver) = bounded_command_queue(8);
        let mut mixer = RealtimeMixer::new(2, receiver);
        let microphone = [0.25, -0.25, 0.5, -0.5];
        let soundboard = [0.5, 0.25, -0.25, 0.5];
        let mut output = [0.0; 4];

        mixer.process(&microphone, &soundboard, &mut output);

        assert_eq!(output, [0.75, 0.0, 0.25, 0.0]);
        assert_eq!(mixer.stats().clipped_samples, 0);
    }

    #[test]
    fn applies_commands_and_reports_clipping() {
        let (sender, receiver) = bounded_command_queue(8);
        let mut mixer = RealtimeMixer::new(1, receiver);
        sender
            .try_send(MixerCommand::SetMuted {
                bus: MixBus::Microphone,
                muted: true,
            })
            .expect("command should fit");
        let soundboard = [2.0; 4];
        let mut output = [0.0; 4];

        mixer.process(&[0.5; 4], &soundboard, &mut output);

        assert_eq!(output, [1.0; 4]);
        assert_eq!(mixer.stats().clipped_samples, 4);
    }
}
