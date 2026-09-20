mod capture;
mod command;
mod dsp;
mod engine;
mod ipc;
mod mixer;
#[cfg(windows)]
mod monitor;
#[cfg(windows)]
mod output;
mod playback;
mod quality;
mod resampler;
#[cfg(windows)]
mod virtual_sink;

#[cfg(windows)]
mod wasapi;

pub use capture::{
    AudioDevice, AudioFormat, CaptureError, CaptureSource, DeviceCatalog, SampleEncoding,
};
pub use command::{CommandReceiver, CommandSender, QueueFull, bounded_command_queue};
pub use dsp::{DspError, PlaybackDspProfile, process_playback_dsp};
pub use engine::{AudioEngine, EngineError, EngineState, EngineStatus};
pub use ipc::{
    CHANNELS, DEFAULT_CAPACITY_FRAMES, HEADER_SIZE, IpcError, IpcHealth, RingWriteResult,
    SAMPLE_RATE, SharedRingWriter,
};
pub use mixer::{MixBus, MixStats, MixerCommand, RealtimeMixer};
#[cfg(windows)]
pub use monitor::MonitorStatus;
#[cfg(windows)]
pub use output::output_device_names;
pub use playback::{PlaybackId, ReplayPolicy};
pub use quality::{AudioQualityReport, run_reference_quality_probe};
pub use resampler::{ResampleError, ResampleResult, WindowedSincResampler};
#[cfg(windows)]
pub use virtual_sink::{
    VirtualSink, VirtualSinkStatus, is_virtual_cable, virtual_cable_candidates,
};

#[cfg(windows)]
pub use wasapi::{SystemDeviceCatalog, WasapiCaptureSource};
