use std::f64::consts::PI;

use thiserror::Error;

const DEFAULT_TAPS: usize = 32;
const DEFAULT_PHASES: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResampleResult {
    pub input_frames_accepted: usize,
    pub output_frames_written: usize,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ResampleError {
    #[error("sample rates and channel count must be non-zero")]
    InvalidConfiguration,
    #[error("input must contain complete interleaved frames")]
    IncompleteInputFrame,
    #[error("resampler input capacity exceeded")]
    CapacityExceeded,
}

/// Streaming polyphase windowed-sinc resampler with preallocated storage.
///
/// Construction allocates the coefficient table and bounded sample FIFO. `process` performs no
/// allocation and introduces a fixed half-kernel latency so adjacent packets remain continuous.
pub struct WindowedSincResampler {
    input_rate: u32,
    output_rate: u32,
    channels: usize,
    taps: usize,
    phases: usize,
    coefficients: Vec<f32>,
    samples: Vec<f32>,
    buffered_frames: usize,
    source_position: f64,
}

impl WindowedSincResampler {
    pub fn new(
        input_rate: u32,
        output_rate: u32,
        channels: usize,
        maximum_input_frames: usize,
    ) -> Result<Self, ResampleError> {
        if input_rate == 0 || output_rate == 0 || channels == 0 || maximum_input_frames == 0 {
            return Err(ResampleError::InvalidConfiguration);
        }

        let taps = DEFAULT_TAPS;
        let phases = DEFAULT_PHASES;
        let history_frames = taps;
        let capacity_frames = maximum_input_frames
            .checked_mul(2)
            .and_then(|frames| frames.checked_add(history_frames * 2))
            .ok_or(ResampleError::CapacityExceeded)?;
        let mut resampler = Self {
            input_rate,
            output_rate,
            channels,
            taps,
            phases,
            coefficients: vec![0.0; taps * phases],
            samples: vec![0.0; capacity_frames * channels],
            buffered_frames: taps / 2,
            source_position: (taps / 2) as f64,
        };
        resampler.build_coefficients();
        Ok(resampler)
    }

    pub const fn latency_frames(&self) -> usize {
        self.taps / 2
    }

    pub fn process(
        &mut self,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<ResampleResult, ResampleError> {
        if !input.len().is_multiple_of(self.channels) {
            return Err(ResampleError::IncompleteInputFrame);
        }
        let input_frames = input.len() / self.channels;
        let capacity_frames = self.samples.len() / self.channels;
        if self.buffered_frames + input_frames > capacity_frames {
            return Err(ResampleError::CapacityExceeded);
        }

        let append_start = self.buffered_frames * self.channels;
        self.samples[append_start..append_start + input.len()].copy_from_slice(input);
        self.buffered_frames += input_frames;

        let output_capacity_frames = output.len() / self.channels;
        let half = self.taps / 2;
        let ratio = self.input_rate as f64 / self.output_rate as f64;
        let mut output_frames = 0;

        while output_frames < output_capacity_frames
            && self.source_position.floor() as usize + half < self.buffered_frames
        {
            let center = self.source_position.floor() as isize;
            let fraction = self.source_position - center as f64;
            let phase = ((fraction * self.phases as f64).round() as usize).min(self.phases - 1);
            let coefficients = &self.coefficients[phase * self.taps..(phase + 1) * self.taps];

            for channel in 0..self.channels {
                let mut sum = 0.0_f32;
                for (tap, coefficient) in coefficients.iter().enumerate() {
                    let frame = center + tap as isize - half as isize + 1;
                    if frame >= 0 && frame < self.buffered_frames as isize {
                        sum += self.samples[frame as usize * self.channels + channel] * coefficient;
                    }
                }
                output[output_frames * self.channels + channel] = sum;
            }

            output_frames += 1;
            self.source_position += ratio;
        }

        self.compact();
        Ok(ResampleResult {
            input_frames_accepted: input_frames,
            output_frames_written: output_frames,
        })
    }

    fn compact(&mut self) {
        let half = self.taps / 2;
        let consumed = (self.source_position.floor() as usize).saturating_sub(half);
        if consumed == 0 {
            return;
        }

        let consumed_samples = consumed * self.channels;
        let retained_samples = (self.buffered_frames - consumed) * self.channels;
        self.samples
            .copy_within(consumed_samples..consumed_samples + retained_samples, 0);
        self.buffered_frames -= consumed;
        self.source_position -= consumed as f64;
    }

    fn build_coefficients(&mut self) {
        let cutoff = (self.output_rate as f64 / self.input_rate as f64).min(1.0) * 0.95;
        let half = self.taps as f64 / 2.0;
        for phase in 0..self.phases {
            let fraction = phase as f64 / self.phases as f64;
            let row = &mut self.coefficients[phase * self.taps..(phase + 1) * self.taps];
            let mut sum = 0.0_f64;
            for (tap, coefficient) in row.iter_mut().enumerate() {
                let x = tap as f64 - half + 1.0 - fraction;
                let sinc = if x.abs() < f64::EPSILON {
                    cutoff
                } else {
                    (PI * x * cutoff).sin() / (PI * x)
                };
                let window_position = tap as f64 / (self.taps - 1) as f64;
                let blackman = 0.42 - 0.5 * (2.0 * PI * window_position).cos()
                    + 0.08 * (4.0 * PI * window_position).cos();
                let value = sinc * blackman;
                *coefficient = value as f32;
                sum += value;
            }
            for coefficient in row {
                *coefficient /= sum as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::WindowedSincResampler;

    #[test]
    fn preserves_a_reference_tone_while_resampling() {
        let input_rate = 44_100;
        let output_rate = 48_000;
        let frequency = 1_000.0;
        let input: Vec<f32> = (0..input_rate)
            .map(|frame| (TAU * frequency * frame as f32 / input_rate as f32).sin() * 0.5)
            .collect();
        let mut output = vec![0.0; output_rate as usize + 128];
        let mut resampler = WindowedSincResampler::new(input_rate, output_rate, 1, input.len())
            .expect("resampler should be configured");

        let result = resampler
            .process(&input, &mut output)
            .expect("resampling should succeed");
        let signal = &output[128..result.output_frames_written.saturating_sub(128)];
        let rms =
            (signal.iter().map(|sample| sample * sample).sum::<f32>() / signal.len() as f32).sqrt();

        assert!((rms - 0.353_553).abs() < 0.003, "unexpected RMS {rms}");
    }

    #[test]
    fn accepts_adjacent_packets_without_discontinuity() {
        let mut resampler =
            WindowedSincResampler::new(48_000, 48_000, 1, 512).expect("valid resampler");
        let first = vec![0.25; 256];
        let second = vec![0.25; 256];
        let mut output = vec![0.0; 1_024];

        let first_result = resampler
            .process(&first, &mut output)
            .expect("first packet should process");
        let second_result = resampler
            .process(&second, &mut output[first_result.output_frames_written..])
            .expect("second packet should process");
        let written = first_result.output_frames_written + second_result.output_frames_written;

        assert!(
            output[32..written]
                .iter()
                .all(|sample| (*sample - 0.25).abs() < 0.000_1)
        );
    }
}
