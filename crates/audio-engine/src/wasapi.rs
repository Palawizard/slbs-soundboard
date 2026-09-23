use std::{marker::PhantomData, rc::Rc, time::Duration};

use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK, DEVICE_STATE_ACTIVE, IAudioCaptureClient,
            IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM,
            WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eCapture, eCommunications,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
                CoUninitialize, STGM_READ,
            },
            Threading::{
                CreateEventW, GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
                WaitForSingleObject,
            },
        },
    },
    core::{GUID, HRESULT, PCWSTR, PWSTR},
};

use crate::{AudioDevice, AudioFormat, CaptureError, CaptureSource, DeviceCatalog, SampleEncoding};

/// Raises the calling thread above game and application threads, as `cpal` does
/// for its output streams, so capture is not starved under heavy CPU load.
pub(crate) fn boost_current_thread_priority() {
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };
}

const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;
const PCM_SUBFORMAT: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
const FLOAT_SUBFORMAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

/// Returned by `CoInitializeEx` when the thread already belongs to an apartment
/// of a different model.
const RPC_E_CHANGED_MODE: HRESULT = HRESULT(0x8001_0106_u32 as i32);

struct ComApartment {
    owned: bool,
}

impl ComApartment {
    /// Joins the thread's COM apartment, asking for the multi-threaded model.
    ///
    /// Another component may already have placed this thread in a
    /// single-threaded apartment: the Windows file dialog requires one, and
    /// `cpal` creates one on whatever thread enumerates devices. Windows reports
    /// that as `RPC_E_CHANGED_MODE`, which is not a failure — COM remains usable
    /// for device enumeration and activation. The apartment then belongs to its
    /// creator, so it must not be uninitialized here.
    fn initialize() -> Result<Self, CaptureError> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self { owned: false });
        }
        result.ok().map_err(platform_error)?;
        Ok(Self { owned: true })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub struct SystemDeviceCatalog;

impl SystemDeviceCatalog {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for SystemDeviceCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceCatalog for SystemDeviceCatalog {
    fn capture_devices(&self) -> Result<Vec<AudioDevice>, CaptureError> {
        let _apartment = ComApartment::initialize()?;
        let enumerator = device_enumerator()?;
        let default_id = device_id(&default_device(&enumerator)?)?;
        let collection = unsafe { enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) }
            .map_err(platform_error)?;

        let count = unsafe { collection.GetCount() }.map_err(platform_error)?;
        let mut devices = Vec::with_capacity(count as usize);
        for index in 0..count {
            let device = unsafe { collection.Item(index) }.map_err(platform_error)?;
            let id = device_id(&device)?;
            devices.push(AudioDevice {
                name: device_name(&device).unwrap_or_else(|_| id.clone()),
                is_default: id == default_id,
                id,
            });
        }

        Ok(devices)
    }

    fn default_capture_device(&self) -> Result<AudioDevice, CaptureError> {
        let _apartment = ComApartment::initialize()?;
        let enumerator = device_enumerator()?;
        let device = default_device(&enumerator)?;
        let id = device_id(&device)?;
        Ok(AudioDevice {
            name: device_name(&device).unwrap_or_else(|_| id.clone()),
            id,
            is_default: true,
        })
    }
}

pub struct WasapiCaptureSource {
    _apartment: ComApartment,
    _thread_affinity: PhantomData<Rc<()>>,
    audio_client: IAudioClient,
    capture_client: IAudioCaptureClient,
    event: OwnedHandle,
    format: AudioFormat,
    packet_capacity_frames: usize,
    started: bool,
}

impl WasapiCaptureSource {
    pub fn open(device_id: Option<&str>) -> Result<Self, CaptureError> {
        let apartment = ComApartment::initialize()?;
        let enumerator = device_enumerator()?;
        let device = match device_id {
            Some(id) => {
                let id_wide = wide_null(id);
                unsafe { enumerator.GetDevice(PCWSTR(id_wide.as_ptr())) }
                    .map_err(|_| CaptureError::DeviceNotFound(id.to_owned()))?
            }
            None => default_device(&enumerator)?,
        };

        let audio_client: IAudioClient =
            unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(platform_error)?;
        let mix_format = unsafe { audio_client.GetMixFormat() }.map_err(platform_error)?;
        let format = unsafe { parse_wave_format(mix_format) };
        let format = match format {
            Ok(format) => format,
            Err(error) => {
                unsafe { CoTaskMemFree(Some(mix_format.cast())) };
                return Err(error);
            }
        };

        let event =
            OwnedHandle(unsafe { CreateEventW(None, false, false, None) }.map_err(platform_error)?);
        let initialize_result = unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                0,
                0,
                mix_format,
                None,
            )
        };
        unsafe { CoTaskMemFree(Some(mix_format.cast())) };
        initialize_result.map_err(platform_error)?;
        unsafe { audio_client.SetEventHandle(event.0) }.map_err(platform_error)?;

        let packet_capacity_frames =
            unsafe { audio_client.GetBufferSize() }.map_err(platform_error)? as usize;
        let capture_client: IAudioCaptureClient =
            unsafe { audio_client.GetService() }.map_err(platform_error)?;

        Ok(Self {
            _apartment: apartment,
            _thread_affinity: PhantomData,
            audio_client,
            capture_client,
            event,
            format,
            packet_capacity_frames,
            started: false,
        })
    }
}

impl CaptureSource for WasapiCaptureSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn maximum_packet_frames(&self) -> usize {
        self.packet_capacity_frames
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        if !self.started {
            unsafe { self.audio_client.Start() }.map_err(platform_error)?;
            self.started = true;
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if self.started {
            unsafe { self.audio_client.Stop() }.map_err(platform_error)?;
            self.started = false;
        }
        Ok(())
    }

    fn read_packet(
        &mut self,
        output: &mut [f32],
        timeout: Duration,
    ) -> Result<usize, CaptureError> {
        if !self.started {
            return Err(CaptureError::NotStarted);
        }

        let required_samples = self
            .format
            .samples_for_frames(self.packet_capacity_frames)
            .ok_or_else(|| CaptureError::Platform("capture capacity overflow".to_owned()))?;
        if output.len() < required_samples {
            return Err(CaptureError::BufferTooSmall {
                required_samples,
                provided_samples: output.len(),
            });
        }

        // Packets left over after a stall are read without waiting for another
        // event; otherwise the backlog never drains and the device buffer overflows.
        let mut packet_frames =
            unsafe { self.capture_client.GetNextPacketSize() }.map_err(platform_error)?;
        if packet_frames == 0 {
            let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
            match unsafe { WaitForSingleObject(self.event.0, timeout_ms) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => return Ok(0),
                result => {
                    return Err(CaptureError::Platform(format!(
                        "audio event wait failed with status {}",
                        result.0
                    )));
                }
            }
            packet_frames =
                unsafe { self.capture_client.GetNextPacketSize() }.map_err(platform_error)?;
        }
        if packet_frames == 0 {
            return Ok(0);
        }

        let mut data = std::ptr::null_mut();
        let mut frames = 0;
        let mut flags = 0;
        unsafe {
            self.capture_client
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
        }
        .map_err(platform_error)?;

        let decode_result = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
            let samples = self.format.samples_for_frames(frames as usize).unwrap_or(0);
            output[..samples].fill(0.0);
            Ok(())
        } else if data.is_null() {
            Err(CaptureError::Platform(
                "WASAPI returned a null non-silent packet".to_owned(),
            ))
        } else {
            unsafe { decode_interleaved(data, frames as usize, self.format, output) }
        };

        let release_result = unsafe { self.capture_client.ReleaseBuffer(frames) };
        decode_result?;
        release_result.map_err(platform_error)?;
        Ok(frames as usize)
    }
}

impl Drop for WasapiCaptureSource {
    fn drop(&mut self) {
        if self.started {
            let _ = unsafe { self.audio_client.Stop() };
        }
    }
}

fn device_enumerator() -> Result<IMMDeviceEnumerator, CaptureError> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(platform_error)
}

fn default_device(enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice, CaptureError> {
    unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) }.map_err(platform_error)
}

fn device_id(device: &IMMDevice) -> Result<String, CaptureError> {
    let id: PWSTR = unsafe { device.GetId() }.map_err(platform_error)?;
    let value =
        unsafe { id.to_string() }.map_err(|error| CaptureError::Platform(error.to_string()));
    unsafe { CoTaskMemFree(Some(id.0.cast())) };
    value
}

fn device_name(device: &IMMDevice) -> Result<String, CaptureError> {
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }.map_err(platform_error)?;
    let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName) }.map_err(platform_error)?;
    Ok(value.to_string())
}

unsafe fn parse_wave_format(format: *const WAVEFORMATEX) -> Result<AudioFormat, CaptureError> {
    if format.is_null() {
        return Err(CaptureError::UnsupportedFormat(
            "WASAPI returned a null mix format".to_owned(),
        ));
    }

    let base = unsafe { std::ptr::read_unaligned(format) };
    let format_tag = base.wFormatTag;
    let bits_per_sample = base.wBitsPerSample;
    let extension_size = base.cbSize;
    let (encoding, valid_bits_per_sample) = match format_tag {
        tag if tag == WAVE_FORMAT_PCM as u16 => (SampleEncoding::SignedInteger, bits_per_sample),
        WAVE_FORMAT_IEEE_FLOAT => (SampleEncoding::Float, bits_per_sample),
        WAVE_FORMAT_EXTENSIBLE => {
            if extension_size < 22 {
                return Err(CaptureError::UnsupportedFormat(format!(
                    "invalid extensible format size {}",
                    extension_size
                )));
            }
            let extended =
                unsafe { std::ptr::read_unaligned(format.cast::<WAVEFORMATEXTENSIBLE>()) };
            let valid_bits = unsafe { extended.Samples.wValidBitsPerSample };
            let subformat = extended.SubFormat;
            let encoding = if subformat == FLOAT_SUBFORMAT {
                SampleEncoding::Float
            } else if subformat == PCM_SUBFORMAT {
                SampleEncoding::SignedInteger
            } else {
                return Err(CaptureError::UnsupportedFormat(format!(
                    "unsupported extensible subformat {:?}",
                    subformat
                )));
            };
            (encoding, valid_bits)
        }
        tag => {
            return Err(CaptureError::UnsupportedFormat(format!(
                "unsupported wave format tag {tag:#06x}"
            )));
        }
    };

    let result = AudioFormat {
        sample_rate: base.nSamplesPerSec,
        channels: base.nChannels,
        bits_per_sample,
        valid_bits_per_sample,
        encoding,
    };
    validate_format(result)?;
    Ok(result)
}

fn validate_format(format: AudioFormat) -> Result<(), CaptureError> {
    if format.sample_rate == 0 || format.channels == 0 {
        return Err(CaptureError::UnsupportedFormat(
            "sample rate and channel count must be non-zero".to_owned(),
        ));
    }
    match (format.encoding, format.bits_per_sample) {
        (SampleEncoding::Float, 32) => Ok(()),
        (SampleEncoding::SignedInteger, 16 | 24 | 32) => Ok(()),
        (encoding, bits) => Err(CaptureError::UnsupportedFormat(format!(
            "{encoding:?} with {bits} bits"
        ))),
    }
}

unsafe fn decode_interleaved(
    data: *const u8,
    frames: usize,
    format: AudioFormat,
    output: &mut [f32],
) -> Result<(), CaptureError> {
    let sample_count = format
        .samples_for_frames(frames)
        .ok_or_else(|| CaptureError::Platform("packet sample count overflow".to_owned()))?;
    let bytes_per_sample = (format.bits_per_sample / 8) as usize;

    for (index, output_sample) in output[..sample_count].iter_mut().enumerate() {
        let sample = unsafe { data.add(index * bytes_per_sample) };
        *output_sample = match (format.encoding, format.bits_per_sample) {
            (SampleEncoding::Float, 32) => {
                f32::from_bits(unsafe { std::ptr::read_unaligned(sample.cast::<u32>()) })
            }
            (SampleEncoding::SignedInteger, 16) => {
                (unsafe { std::ptr::read_unaligned(sample.cast::<i16>()) }) as f32 / 32_768.0
            }
            (SampleEncoding::SignedInteger, 24) => {
                let bytes = unsafe { std::slice::from_raw_parts(sample, 3) };
                let value =
                    ((bytes[0] as i32) | ((bytes[1] as i32) << 8) | ((bytes[2] as i32) << 16)) << 8
                        >> 8;
                value as f32 / 8_388_608.0
            }
            (SampleEncoding::SignedInteger, 32) => {
                let value = unsafe { std::ptr::read_unaligned(sample.cast::<i32>()) };
                value as f32 / 2_147_483_648.0
            }
            _ => {
                return Err(CaptureError::UnsupportedFormat(
                    "packet encoding".to_owned(),
                ));
            }
        };
    }
    Ok(())
}

fn platform_error(error: windows::core::Error) -> CaptureError {
    CaptureError::Platform(error.to_string())
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::{decode_interleaved, parse_wave_format};
    use crate::{AudioFormat, SampleEncoding};
    use windows::Win32::Media::Audio::{WAVE_FORMAT_PCM, WAVEFORMATEX};
    use super::{ComApartment, RPC_E_CHANGED_MODE};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

    #[test]
    fn changed_mode_matches_the_documented_hresult() {
        assert_eq!(RPC_E_CHANGED_MODE.0, -2_147_417_850);
    }

    #[test]
    fn joins_a_foreign_single_threaded_apartment_without_owning_it() {
        // Reproduces the conflict seen in the application: the file dialog and
        // cpal both leave the calling thread in a single-threaded apartment,
        // after which asking for the multi-threaded model returns
        // RPC_E_CHANGED_MODE. Device enumeration must keep working.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .expect("the test thread should enter a single-threaded apartment");
        let apartment =
            ComApartment::initialize().expect("a foreign apartment is not a failure");
        assert!(
            !apartment.owned,
            "an apartment created elsewhere must not be uninitialized here"
        );
        drop(apartment);
        unsafe { CoUninitialize() };
    }

    #[test]
    fn parses_pcm_wave_format() {
        let wave = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: 2,
            nSamplesPerSec: 48_000,
            nAvgBytesPerSec: 192_000,
            nBlockAlign: 4,
            wBitsPerSample: 16,
            cbSize: 0,
        };

        let parsed = unsafe { parse_wave_format(&wave) }.expect("format should parse");
        assert_eq!(
            parsed,
            AudioFormat {
                sample_rate: 48_000,
                channels: 2,
                bits_per_sample: 16,
                valid_bits_per_sample: 16,
                encoding: SampleEncoding::SignedInteger,
            }
        );
    }

    #[test]
    fn decodes_signed_24_bit_pcm_without_allocation() {
        let input = [0x00, 0x00, 0x80, 0xff, 0xff, 0x7f];
        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 1,
            bits_per_sample: 24,
            valid_bits_per_sample: 24,
            encoding: SampleEncoding::SignedInteger,
        };
        let mut output = [0.0; 2];

        unsafe { decode_interleaved(input.as_ptr(), 2, format, &mut output) }
            .expect("samples should decode");

        assert_eq!(output[0], -1.0);
        assert!((output[1] - (1.0 - 1.0 / 8_388_608.0)).abs() < f32::EPSILON);
    }
}
