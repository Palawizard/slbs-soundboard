use std::time::Duration;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleEncoding {
    Float,
    SignedInteger,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub valid_bits_per_sample: u16,
    pub encoding: SampleEncoding,
}

impl AudioFormat {
    pub fn samples_for_frames(self, frames: usize) -> Option<usize> {
        frames.checked_mul(self.channels as usize)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("audio capture is not available on this platform")]
    UnsupportedPlatform,
    #[error("audio device was not found: {0}")]
    DeviceNotFound(String),
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("capture buffer needs at least {required_samples} samples, got {provided_samples}")]
    BufferTooSmall {
        required_samples: usize,
        provided_samples: usize,
    },
    #[error("audio capture has not been started")]
    NotStarted,
    #[error("Windows audio error: {0}")]
    Platform(String),
}

pub trait DeviceCatalog {
    fn capture_devices(&self) -> Result<Vec<AudioDevice>, CaptureError>;
    fn default_capture_device(&self) -> Result<AudioDevice, CaptureError>;
}

pub trait CaptureSource {
    fn format(&self) -> AudioFormat;
    fn maximum_packet_frames(&self) -> usize;
    fn start(&mut self) -> Result<(), CaptureError>;
    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Reads one complete interleaved packet into a caller-owned buffer.
    ///
    /// The buffer must hold `maximum_packet_frames() * format().channels` samples.
    /// Returning zero means that the timeout elapsed without an audio packet.
    fn read_packet(&mut self, output: &mut [f32], timeout: Duration)
    -> Result<usize, CaptureError>;
}

#[cfg(test)]
mod tests {
    use super::{AudioFormat, SampleEncoding};

    #[test]
    fn calculates_interleaved_sample_capacity() {
        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 32,
            valid_bits_per_sample: 32,
            encoding: SampleEncoding::Float,
        };

        assert_eq!(format.samples_for_frames(480), Some(960));
    }
}
