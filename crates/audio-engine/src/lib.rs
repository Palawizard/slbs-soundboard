mod capture;

#[cfg(windows)]
mod wasapi;

pub use capture::{
    AudioDevice, AudioFormat, CaptureError, CaptureSource, DeviceCatalog, SampleEncoding,
};

#[cfg(windows)]
pub use wasapi::{SystemDeviceCatalog, WasapiCaptureSource};
