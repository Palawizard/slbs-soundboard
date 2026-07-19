use std::time::Duration;

use crate::{RealtimeMixer, WindowedSincResampler, bounded_command_queue};

const INPUT_RATE: u32 = 44_100;
const OUTPUT_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const BLOCK_FRAMES: usize = 441;
const OUTPUT_CAPACITY_FRAMES: usize = 544;
const TEST_FREQUENCY_HZ: f64 = 997.0;
const TEST_LEVEL: f64 = 0.25;

#[derive(Clone, Copy, Debug)]
pub struct AudioQualityReport {
    pub resampler_latency_frames: usize,
    pub tone_gain_db: f64,
    pub frequency_error_hz: f64,
    pub clipped_samples: u64,
    pub dropout_frames: u64,
    pub simulated_frames: u64,
}

impl AudioQualityReport {
    pub fn passes(&self) -> bool {
        self.resampler_latency_frames <= 32
            && self.tone_gain_db.abs() <= 0.1
            && self.frequency_error_hz <= 1.0
            && self.clipped_samples == 0
            && self.dropout_frames == 0
    }
}

pub fn run_reference_quality_probe(simulated_duration: Duration) -> AudioQualityReport {
    let (mixer_sender, mixer_receiver) = bounded_command_queue(8);
    drop(mixer_sender);
    let mut mixer = RealtimeMixer::new(CHANNELS, mixer_receiver);
    let mut resampler = WindowedSincResampler::new(INPUT_RATE, OUTPUT_RATE, CHANNELS, BLOCK_FRAMES)
        .expect("quality probe configuration is valid");
    let latency = resampler.latency_frames();
    let input_blocks = (simulated_duration.as_secs_f64() * INPUT_RATE as f64 / BLOCK_FRAMES as f64)
        .ceil() as usize;

    let mut input = [0.0_f32; BLOCK_FRAMES * CHANNELS];
    let mut resampled = [0.0_f32; OUTPUT_CAPACITY_FRAMES * CHANNELS];
    let soundboard = [0.0_f32; OUTPUT_CAPACITY_FRAMES * CHANNELS];
    let mut mixed = [0.0_f32; OUTPUT_CAPACITY_FRAMES * CHANNELS];
    let mut input_frame = 0_u64;
    let mut metrics = SignalAccumulator::default();

    for _ in 0..input_blocks {
        for frame in input.chunks_exact_mut(CHANNELS) {
            let phase =
                std::f64::consts::TAU * TEST_FREQUENCY_HZ * input_frame as f64 / INPUT_RATE as f64;
            let sample = (phase.sin() * TEST_LEVEL) as f32;
            frame.fill(sample);
            input_frame += 1;
        }
        let result = resampler
            .process(&input, &mut resampled)
            .expect("quality probe resampling must remain bounded");
        let samples = result.output_frames_written * CHANNELS;
        mixer.process(
            &resampled[..samples],
            &soundboard[..samples],
            &mut mixed[..samples],
        );
        metrics.observe(&mixed[..samples]);
    }

    let measured_frequency = metrics.frequency_hz(OUTPUT_RATE);
    AudioQualityReport {
        resampler_latency_frames: latency,
        tone_gain_db: 20.0 * (metrics.rms() / (TEST_LEVEL / 2.0_f64.sqrt())).log10(),
        frequency_error_hz: (measured_frequency - TEST_FREQUENCY_HZ).abs(),
        clipped_samples: mixer.stats().clipped_samples,
        dropout_frames: metrics.dropout_frames,
        simulated_frames: metrics.frames,
    }
}

#[derive(Default)]
struct SignalAccumulator {
    sum_squares: f64,
    samples: u64,
    frames: u64,
    positive_crossings: u64,
    previous_left: f32,
    dropout_frames: u64,
}

impl SignalAccumulator {
    fn observe(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(CHANNELS) {
            let left = frame[0];
            if self.previous_left <= 0.0 && left > 0.0 && self.frames > 0 {
                self.positive_crossings += 1;
            }
            if left == 0.0 && frame[1] == 0.0 && self.frames > OUTPUT_RATE as u64 / 10 {
                self.dropout_frames += 1;
            }
            self.previous_left = left;
            self.sum_squares += frame
                .iter()
                .map(|sample| f64::from(*sample).powi(2))
                .sum::<f64>();
            self.samples += CHANNELS as u64;
            self.frames += 1;
        }
    }

    fn rms(&self) -> f64 {
        (self.sum_squares / self.samples.max(1) as f64).sqrt()
    }

    fn frequency_hz(&self, sample_rate: u32) -> f64 {
        self.positive_crossings as f64 * sample_rate as f64 / self.frames.max(1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_path_meets_frequency_gain_and_dropout_thresholds() {
        let report = run_reference_quality_probe(Duration::from_secs(5));
        assert!(
            report.passes(),
            "reference quality thresholds failed: {report:?}"
        );
    }

    #[test]
    fn ten_minutes_of_virtual_time_remain_bit_stable_without_clipping() {
        let (_sender, receiver) = bounded_command_queue(2);
        let mut mixer = RealtimeMixer::new(CHANNELS, receiver);
        let microphone = [0.125_f32; 480 * CHANNELS];
        let soundboard = [-0.0625_f32; 480 * CHANNELS];
        let mut output = [0.0_f32; 480 * CHANNELS];
        let blocks = Duration::from_secs(10 * 60).as_millis() / 10;

        for _ in 0..blocks {
            mixer.process(&microphone, &soundboard, &mut output);
            assert!(output.iter().all(|sample| *sample == 0.0625));
        }

        assert_eq!(mixer.stats().processed_frames, 10 * 60 * OUTPUT_RATE as u64);
        assert_eq!(mixer.stats().clipped_samples, 0);
    }
}
