use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};

/// A source of interleaved stereo frames consumed by a `cpal` output stream.
pub(crate) trait FrameSource: Send + 'static {
    fn next_frame(&mut self) -> [f32; 2];
}

/// Names of every output device the default host currently exposes.
pub fn output_device_names() -> Vec<String> {
    let Ok(devices) = cpal::default_host().output_devices() else {
        return Vec::new();
    };
    let mut names: Vec<String> = devices.filter_map(|device| device.name().ok()).collect();
    names.sort();
    names.dedup();
    names
}

/// Resolves an output device by its exact name, then by a case-insensitive prefix.
pub(crate) fn find_output_device(name: &str) -> Option<Device> {
    let devices: Vec<Device> = cpal::default_host().output_devices().ok()?.collect();
    if let Some(device) = devices
        .iter()
        .find(|device| device.name().is_ok_and(|current| current == name))
    {
        return Some(device.clone());
    }
    let wanted = name.to_lowercase();
    devices.into_iter().find(|device| {
        device
            .name()
            .is_ok_and(|current| current.to_lowercase().starts_with(&wanted))
    })
}

pub(crate) fn build_output_stream<S: FrameSource>(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    mut source: S,
    error_sender: std::sync::mpsc::Sender<String>,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let error_callback = move |error: cpal::StreamError| {
        let _ = error_sender.send(error.to_string());
    };
    match sample_format {
        SampleFormat::F32 => device
            .build_output_stream(
                config,
                move |output: &mut [f32], _| {
                    write_frames(output, channels, &mut source, |sample| sample)
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::I16 => device
            .build_output_stream(
                config,
                move |output: &mut [i16], _| {
                    write_frames(output, channels, &mut source, |sample| {
                        (sample * i16::MAX as f32).round() as i16
                    })
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::U16 => device
            .build_output_stream(
                config,
                move |output: &mut [u16], _| {
                    write_frames(output, channels, &mut source, |sample| {
                        (((sample + 1.0) * 0.5) * u16::MAX as f32).round() as u16
                    })
                },
                error_callback,
                None,
            )
            .map_err(|error| error.to_string()),
        _ => Err(format!(
            "Le format de sortie {sample_format:?} n’est pas pris en charge."
        )),
    }
}

pub(crate) fn write_frames<T: Copy, S: FrameSource>(
    output: &mut [T],
    channels: usize,
    source: &mut S,
    convert: impl Fn(f32) -> T,
) {
    for output_frame in output.chunks_exact_mut(channels.max(1)) {
        let frame = source.next_frame();
        for (channel, sample) in output_frame.iter_mut().enumerate() {
            *sample = convert(if channel == 0 {
                frame[0]
            } else if channel == 1 {
                frame[1]
            } else {
                (frame[0] + frame[1]) * 0.5
            });
        }
    }
}
