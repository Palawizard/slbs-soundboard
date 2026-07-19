use thiserror::Error;

use crate::{CHANNELS, SAMPLE_RATE, WindowedSincResampler};

const MIN_PITCH_SEMITONES: f32 = -12.0;
const MAX_PITCH_SEMITONES: f32 = 12.0;
const MIN_SPEED: f32 = 0.5;
const MAX_SPEED: f32 = 2.0;
const GRAIN_FRAMES: usize = 2_048;
const ANALYSIS_HOP: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaybackDspProfile {
    pub pitch_semitones: f32,
    pub speed: f32,
}

impl PlaybackDspProfile {
    pub fn new(pitch_semitones: f32, speed: f32) -> Result<Self, DspError> {
        if !pitch_semitones.is_finite()
            || !speed.is_finite()
            || !(MIN_PITCH_SEMITONES..=MAX_PITCH_SEMITONES).contains(&pitch_semitones)
            || !(MIN_SPEED..=MAX_SPEED).contains(&speed)
        {
            return Err(DspError::InvalidProfile);
        }
        Ok(Self {
            pitch_semitones,
            speed,
        })
    }

    fn pitch_ratio(self) -> f32 {
        2.0_f32.powf(self.pitch_semitones / 12.0)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum DspError {
    #[error("pitch and speed are outside their safe ranges")]
    InvalidProfile,
    #[error("audio must contain complete stereo frames")]
    InvalidAudio,
    #[error("audio processing failed")]
    ProcessingFailed,
}

pub fn process_playback_dsp(
    input: &[f32],
    profile: PlaybackDspProfile,
) -> Result<Vec<f32>, DspError> {
    if input.is_empty() || !input.len().is_multiple_of(CHANNELS as usize) {
        return Err(DspError::InvalidAudio);
    }
    PlaybackDspProfile::new(profile.pitch_semitones, profile.speed)?;
    if profile.pitch_semitones.abs() < f32::EPSILON && (profile.speed - 1.0).abs() < f32::EPSILON {
        return Ok(input.to_vec());
    }

    let pitch_ratio = profile.pitch_ratio();
    let pitched = resample_for_pitch(input, pitch_ratio)?;
    let target_frames = ((input.len() / CHANNELS as usize) as f64 / profile.speed as f64)
        .round()
        .max(1.0) as usize;
    let stretch = pitch_ratio / profile.speed;
    let mut output = overlap_add_stretch(&pitched, stretch, target_frames);
    output.truncate(target_frames * CHANNELS as usize);
    output.resize(target_frames * CHANNELS as usize, 0.0);
    normalize_if_needed(&mut output);
    Ok(output)
}

fn resample_for_pitch(input: &[f32], ratio: f32) -> Result<Vec<f32>, DspError> {
    if (ratio - 1.0).abs() < 0.000_01 {
        return Ok(input.to_vec());
    }
    const CHUNK_FRAMES: usize = 4_096;
    let virtual_input_rate = (SAMPLE_RATE as f32 * ratio).round() as u32;
    let mut resampler = WindowedSincResampler::new(
        virtual_input_rate,
        SAMPLE_RATE,
        CHANNELS as usize,
        CHUNK_FRAMES,
    )
    .map_err(|_| DspError::ProcessingFailed)?;
    let input_frames = input.len() / CHANNELS as usize;
    let expected_frames = (input_frames as f64 / ratio as f64).ceil() as usize;
    let output_capacity = ((CHUNK_FRAMES as f64 / ratio as f64).ceil() as usize) + 96;
    let mut scratch = vec![0.0; output_capacity * CHANNELS as usize];
    let mut output = Vec::with_capacity((expected_frames + 96) * CHANNELS as usize);
    for chunk in input.chunks(CHUNK_FRAMES * CHANNELS as usize) {
        let written = resampler
            .process(chunk, &mut scratch)
            .map_err(|_| DspError::ProcessingFailed)?
            .output_frames_written;
        output.extend_from_slice(&scratch[..written * CHANNELS as usize]);
    }
    let silence = [0.0_f32; 64 * CHANNELS as usize];
    let written = resampler
        .process(&silence, &mut scratch)
        .map_err(|_| DspError::ProcessingFailed)?
        .output_frames_written;
    output.extend_from_slice(&scratch[..written * CHANNELS as usize]);
    Ok(output)
}

fn overlap_add_stretch(input: &[f32], stretch: f32, target_frames: usize) -> Vec<f32> {
    let input_frames = input.len() / CHANNELS as usize;
    if input_frames <= GRAIN_FRAMES {
        return linear_resize(input, target_frames);
    }
    let synthesis_hop = ((ANALYSIS_HOP as f32 * stretch).round() as usize).clamp(1, GRAIN_FRAMES);
    let grains = target_frames
        .saturating_sub(GRAIN_FRAMES)
        .div_ceil(synthesis_hop)
        + 1;
    let provisional_frames =
        (grains.saturating_sub(1) * synthesis_hop + GRAIN_FRAMES).max(target_frames);
    let mut output = vec![0.0_f32; provisional_frames * CHANNELS as usize];
    let mut weights = vec![0.0_f32; provisional_frames];

    for grain in 0..grains {
        let output_start = grain * synthesis_hop;
        let expected_input = (grain * ANALYSIS_HOP).min(input_frames - GRAIN_FRAMES);
        let input_start = if grain == 0 {
            expected_input
        } else {
            best_aligned_input(
                input,
                &output,
                &weights,
                expected_input,
                output_start,
                synthesis_hop,
            )
        };
        for frame in 0..GRAIN_FRAMES {
            let window = hann(frame, GRAIN_FRAMES);
            let source = (input_start + frame) * CHANNELS as usize;
            let destination = (output_start + frame) * CHANNELS as usize;
            for channel in 0..CHANNELS as usize {
                output[destination + channel] += input[source + channel] * window;
            }
            weights[output_start + frame] += window;
        }
    }
    for (frame, weight) in weights.into_iter().enumerate() {
        if weight > 0.000_1 {
            for channel in 0..CHANNELS as usize {
                output[frame * CHANNELS as usize + channel] /= weight;
            }
        }
    }
    output.truncate(target_frames * CHANNELS as usize);
    output
}

fn best_aligned_input(
    input: &[f32],
    output: &[f32],
    weights: &[f32],
    expected: usize,
    output_start: usize,
    synthesis_hop: usize,
) -> usize {
    let input_frames = input.len() / CHANNELS as usize;
    let overlap = GRAIN_FRAMES.saturating_sub(synthesis_hop).min(512);
    if overlap < 32 || output_start + overlap > weights.len() {
        return expected;
    }
    let minimum = expected.saturating_sub(128);
    let maximum = (expected + 128).min(input_frames - GRAIN_FRAMES);
    let mut best = expected;
    let mut best_score = f32::NEG_INFINITY;
    for candidate in (minimum..=maximum).step_by(2) {
        let mut dot = 0.0_f32;
        let mut source_energy = 0.0_f32;
        let mut target_energy = 0.0_f32;
        for frame in (0..overlap).step_by(4) {
            let weight = weights[output_start + frame];
            if weight <= 0.000_1 {
                continue;
            }
            let source_index = (candidate + frame) * CHANNELS as usize;
            let target_index = (output_start + frame) * CHANNELS as usize;
            let source = (input[source_index] + input[source_index + 1]) * 0.5;
            let target = (output[target_index] + output[target_index + 1]) * 0.5 / weight;
            dot += source * target;
            source_energy += source * source;
            target_energy += target * target;
        }
        let score = dot / (source_energy * target_energy).sqrt().max(0.000_001);
        if score > best_score {
            best_score = score;
            best = candidate;
        }
    }
    best
}

fn linear_resize(input: &[f32], target_frames: usize) -> Vec<f32> {
    let input_frames = input.len() / CHANNELS as usize;
    let mut output = vec![0.0; target_frames * CHANNELS as usize];
    if input_frames == 1 {
        for frame in output.chunks_exact_mut(CHANNELS as usize) {
            frame.copy_from_slice(&input[..CHANNELS as usize]);
        }
        return output;
    }
    let scale = (input_frames - 1) as f64 / target_frames.saturating_sub(1).max(1) as f64;
    for frame in 0..target_frames {
        let position = frame as f64 * scale;
        let left = position.floor() as usize;
        let right = (left + 1).min(input_frames - 1);
        let fraction = (position - left as f64) as f32;
        for channel in 0..CHANNELS as usize {
            let a = input[left * CHANNELS as usize + channel];
            let b = input[right * CHANNELS as usize + channel];
            output[frame * CHANNELS as usize + channel] = a + (b - a) * fraction;
        }
    }
    output
}

fn hann(position: usize, length: usize) -> f32 {
    let phase = std::f32::consts::TAU * position as f32 / (length - 1) as f32;
    0.5 - 0.5 * phase.cos()
}

fn normalize_if_needed(samples: &mut [f32]) {
    let peak = samples
        .iter()
        .fold(0.0_f32, |value, sample| value.max(sample.abs()));
    if peak > 0.98 {
        let gain = 0.98 / peak;
        for sample in samples {
            *sample *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    fn tone(frequency: f32, seconds: f32) -> Vec<f32> {
        (0..(SAMPLE_RATE as f32 * seconds) as usize)
            .flat_map(|frame| {
                let sample = (TAU * frequency * frame as f32 / SAMPLE_RATE as f32).sin() * 0.5;
                [sample, sample]
            })
            .collect()
    }

    fn estimate_frequency(samples: &[f32]) -> f32 {
        let mono: Vec<f32> = samples.chunks_exact(2).map(|frame| frame[0]).collect();
        let start = mono.len().min(4_096);
        let crossings = mono[start..]
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        crossings as f32 * SAMPLE_RATE as f32 / (mono.len() - start) as f32
    }

    #[test]
    fn rejects_profiles_outside_safe_ranges() {
        assert_eq!(
            PlaybackDspProfile::new(13.0, 1.0),
            Err(DspError::InvalidProfile)
        );
        assert_eq!(
            PlaybackDspProfile::new(0.0, 0.25),
            Err(DspError::InvalidProfile)
        );
    }

    #[test]
    fn changes_speed_without_changing_pitch_materially() {
        let input = tone(440.0, 1.0);
        let output =
            process_playback_dsp(&input, PlaybackDspProfile::new(0.0, 2.0).unwrap()).unwrap();
        assert!((output.len() as f32 / input.len() as f32 - 0.5).abs() < 0.01);
        let frequency = estimate_frequency(&output);
        assert!((frequency - 440.0).abs() < 12.0, "frequency {frequency}");
    }

    #[test]
    fn changes_pitch_while_preserving_requested_duration() {
        let input = tone(440.0, 1.0);
        let output =
            process_playback_dsp(&input, PlaybackDspProfile::new(12.0, 1.0).unwrap()).unwrap();
        assert!((output.len() as f32 / input.len() as f32 - 1.0).abs() < 0.01);
        let frequency = estimate_frequency(&output);
        assert!((frequency - 880.0).abs() < 24.0, "frequency {frequency}");
        assert!(output.iter().all(|sample| sample.abs() <= 0.98));
    }
}
