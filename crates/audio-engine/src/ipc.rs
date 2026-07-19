use std::mem::{align_of, size_of};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use thiserror::Error;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const DEFAULT_CAPACITY_FRAMES: u32 = 16_384;
pub const HEADER_SIZE: usize = 128;
const MAGIC: u32 = u32::from_le_bytes(*b"SLBR");
const VERSION: u16 = 1;
const SAMPLE_FORMAT_F32: u16 = 1;
const FLAG_PRODUCER_ACTIVE: u32 = 1;

#[repr(C, align(64))]
struct RingHeader {
    magic: u32,
    version: u16,
    header_size: u16,
    sample_rate: u32,
    channels: u16,
    sample_format: u16,
    capacity_frames: u32,
    flags: AtomicU32,
    write_frame: AtomicU64,
    read_frame: AtomicU64,
    heartbeat_ms: AtomicU64,
    producer_epoch: u64,
    overrun_frames: AtomicU64,
    underrun_frames: AtomicU64,
    consumed_frames: AtomicU64,
    reserved: [u64; 6],
}

const _: () = assert!(size_of::<RingHeader>() == HEADER_SIZE);
const _: () = assert!(align_of::<RingHeader>() == 64);

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("ring capacity must be a non-zero power of two")]
    InvalidCapacity,
    #[error("audio data must contain complete stereo frames")]
    PartialFrame,
    #[error("shared-memory operation failed: {0}")]
    Platform(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingWriteResult {
    pub written_frames: usize,
    pub dropped_frames: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcHealth {
    pub producer_active: bool,
    pub queued_frames: u64,
    pub overrun_frames: u64,
    pub underrun_frames: u64,
    pub consumed_frames: u64,
    pub heartbeat_ms: u64,
}

struct RingWriterCore {
    base: *mut u8,
    capacity_frames: u32,
}

impl RingWriterCore {
    unsafe fn initialize(base: *mut u8, capacity_frames: u32, epoch: u64) -> Self {
        let header = base.cast::<RingHeader>();
        // SAFETY: the caller provides a writable mapping large enough for the header and samples.
        unsafe {
            header.write(RingHeader {
                magic: MAGIC,
                version: VERSION,
                header_size: HEADER_SIZE as u16,
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
                sample_format: SAMPLE_FORMAT_F32,
                capacity_frames,
                flags: AtomicU32::new(FLAG_PRODUCER_ACTIVE),
                write_frame: AtomicU64::new(0),
                read_frame: AtomicU64::new(0),
                heartbeat_ms: AtomicU64::new(monotonic_ms()),
                producer_epoch: epoch,
                overrun_frames: AtomicU64::new(0),
                underrun_frames: AtomicU64::new(0),
                consumed_frames: AtomicU64::new(0),
                reserved: [0; 6],
            });
        }
        Self {
            base,
            capacity_frames,
        }
    }

    fn header(&self) -> &RingHeader {
        // SAFETY: the mapping remains alive for the lifetime of this core.
        unsafe { &*self.base.cast::<RingHeader>() }
    }

    fn samples(&self) -> *mut f32 {
        // SAFETY: HEADER_SIZE is part of the cross-language layout contract.
        unsafe { self.base.add(HEADER_SIZE).cast::<f32>() }
    }

    fn write(&mut self, interleaved: &[f32]) -> Result<RingWriteResult, IpcError> {
        if !interleaved.len().is_multiple_of(CHANNELS as usize) {
            return Err(IpcError::PartialFrame);
        }

        let requested = interleaved.len() / CHANNELS as usize;
        let header = self.header();
        let write = header.write_frame.load(Ordering::Relaxed);
        let read = header.read_frame.load(Ordering::Acquire);
        let queued = write.saturating_sub(read).min(self.capacity_frames as u64);
        let available = self.capacity_frames as usize - queued as usize;
        let written = requested.min(available);
        let dropped = requested - written;
        let mask = self.capacity_frames as u64 - 1;

        for frame in 0..written {
            let destination_frame = ((write + frame as u64) & mask) as usize;
            let source = frame * CHANNELS as usize;
            let destination = destination_frame * CHANNELS as usize;
            // SAFETY: both indices are bounded by the validated frame counts.
            unsafe {
                self.samples()
                    .add(destination)
                    .copy_from_nonoverlapping(interleaved.as_ptr().add(source), CHANNELS as usize);
            }
        }

        if dropped != 0 {
            header
                .overrun_frames
                .fetch_add(dropped as u64, Ordering::Relaxed);
        }
        header.heartbeat_ms.store(monotonic_ms(), Ordering::Relaxed);
        header.flags.store(FLAG_PRODUCER_ACTIVE, Ordering::Relaxed);
        header
            .write_frame
            .store(write + written as u64, Ordering::Release);

        Ok(RingWriteResult {
            written_frames: written,
            dropped_frames: dropped,
        })
    }

    fn health(&self) -> IpcHealth {
        let header = self.header();
        let write = header.write_frame.load(Ordering::Acquire);
        let read = header.read_frame.load(Ordering::Acquire);
        IpcHealth {
            producer_active: header.flags.load(Ordering::Relaxed) & FLAG_PRODUCER_ACTIVE != 0,
            queued_frames: write.saturating_sub(read).min(self.capacity_frames as u64),
            overrun_frames: header.overrun_frames.load(Ordering::Relaxed),
            underrun_frames: header.underrun_frames.load(Ordering::Relaxed),
            consumed_frames: header.consumed_frames.load(Ordering::Relaxed),
            heartbeat_ms: header.heartbeat_ms.load(Ordering::Relaxed),
        }
    }

    fn deactivate(&self) {
        self.header().flags.store(0, Ordering::Release);
    }
}

pub struct SharedRingWriter {
    core: RingWriterCore,
    #[cfg(windows)]
    mapping: windows::Win32::Foundation::HANDLE,
    #[cfg(windows)]
    view: windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS,
    #[cfg(not(windows))]
    storage: Box<[AlignedBlock]>,
}

#[cfg(not(windows))]
#[repr(C, align(64))]
struct AlignedBlock([u8; 64]);

// The writer is moved to, and exclusively used by, the audio engine thread.
unsafe impl Send for SharedRingWriter {}

impl SharedRingWriter {
    pub fn create(capacity_frames: u32) -> Result<Self, IpcError> {
        if capacity_frames == 0 || !capacity_frames.is_power_of_two() {
            return Err(IpcError::InvalidCapacity);
        }
        create_platform_ring(capacity_frames)
    }

    pub fn write(&mut self, interleaved: &[f32]) -> Result<RingWriteResult, IpcError> {
        self.core.write(interleaved)
    }

    pub fn health(&self) -> IpcHealth {
        self.core.health()
    }
}

#[cfg(windows)]
fn create_platform_ring(capacity_frames: u32) -> Result<SharedRingWriter, IpcError> {
    use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
        VirtualLock,
    };
    use windows::core::w;

    let size = mapping_size(capacity_frames);
    // SAFETY: all handles and pointer lifetimes are retained by SharedRingWriter.
    unsafe {
        let mapping = CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            None,
            PAGE_READWRITE,
            0,
            size as u32,
            w!("SLBVirtualAudioRingV1"),
        )
        .map_err(platform_error)?;
        let view = MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, size);
        if view.Value.is_null() {
            let _ = CloseHandle(mapping);
            return Err(platform_error(windows::core::Error::from_win32()));
        }
        if let Err(error) = VirtualLock(view.Value, size) {
            let _ = UnmapViewOfFile(view);
            let _ = CloseHandle(mapping);
            return Err(platform_error(error));
        }

        let epoch = windows::Win32::System::SystemInformation::GetTickCount64();
        Ok(SharedRingWriter {
            core: RingWriterCore::initialize(view.Value.cast(), capacity_frames, epoch),
            mapping,
            view,
        })
    }
}

#[cfg(not(windows))]
fn create_platform_ring(capacity_frames: u32) -> Result<SharedRingWriter, IpcError> {
    let blocks = mapping_size(capacity_frames).div_ceil(size_of::<AlignedBlock>());
    let mut storage = (0..blocks)
        .map(|_| AlignedBlock([0; 64]))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let core = unsafe {
        RingWriterCore::initialize(storage.as_mut_ptr().cast::<u8>(), capacity_frames, 1)
    };
    Ok(SharedRingWriter { core, storage })
}

#[cfg(windows)]
fn platform_error(error: windows::core::Error) -> IpcError {
    IpcError::Platform(error.to_string())
}

const fn mapping_size(capacity_frames: u32) -> usize {
    HEADER_SIZE + capacity_frames as usize * CHANNELS as usize * size_of::<f32>()
}

#[cfg(windows)]
fn monotonic_ms() -> u64 {
    // SAFETY: GetTickCount64 has no preconditions.
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}

#[cfg(not(windows))]
fn monotonic_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

impl Drop for SharedRingWriter {
    fn drop(&mut self) {
        self.core.deactivate();
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::Memory::{UnmapViewOfFile, VirtualUnlock};
            let _ = VirtualUnlock(self.view.Value, mapping_size(self.core.capacity_frames));
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.mapping);
        }
        #[cfg(not(windows))]
        let _ = self.storage.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SHARED_MAPPING_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn consume(writer: &SharedRingWriter, frames: usize) -> Vec<f32> {
        let header = writer.core.header();
        let read = header.read_frame.load(Ordering::Relaxed);
        let write = header.write_frame.load(Ordering::Acquire);
        let available = (write - read).min(frames as u64) as usize;
        let mask = writer.core.capacity_frames as u64 - 1;
        let mut result = Vec::with_capacity(available * CHANNELS as usize);
        for frame in 0..available {
            let source_frame = ((read + frame as u64) & mask) as usize;
            for channel in 0..CHANNELS as usize {
                // SAFETY: the source index is within the mapping.
                result.push(unsafe {
                    *writer
                        .core
                        .samples()
                        .add(source_frame * CHANNELS as usize + channel)
                });
            }
        }
        header
            .read_frame
            .store(read + available as u64, Ordering::Release);
        header
            .consumed_frames
            .fetch_add(available as u64, Ordering::Relaxed);
        result
    }

    #[test]
    fn layout_matches_driver_contract() {
        assert_eq!(size_of::<RingHeader>(), 128);
        assert_eq!(std::mem::offset_of!(RingHeader, write_frame), 24);
        assert_eq!(std::mem::offset_of!(RingHeader, underrun_frames), 64);
    }

    #[test]
    fn ring_preserves_order_across_wrap() {
        let _guard = SHARED_MAPPING_TEST.lock().unwrap();
        let mut writer = SharedRingWriter::create(4).unwrap();
        writer.write(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
        assert_eq!(consume(&writer, 2), [1.0, 2.0, 3.0, 4.0]);
        writer.write(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0]).unwrap();
        assert_eq!(
            consume(&writer, 4),
            [5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0]
        );
    }

    #[test]
    fn full_ring_drops_new_frames_and_reports_overrun() {
        let _guard = SHARED_MAPPING_TEST.lock().unwrap();
        let mut writer = SharedRingWriter::create(2).unwrap();
        let result = writer.write(&[0.0, 0.0, 1.0, 1.0, 2.0, 2.0]).unwrap();
        assert_eq!(result.written_frames, 2);
        assert_eq!(result.dropped_frames, 1);
        assert_eq!(writer.health().overrun_frames, 1);
    }

    #[test]
    fn rejects_partial_frames_and_invalid_capacity() {
        let _guard = SHARED_MAPPING_TEST.lock().unwrap();
        assert!(matches!(
            SharedRingWriter::create(3),
            Err(IpcError::InvalidCapacity)
        ));
        let mut writer = SharedRingWriter::create(2).unwrap();
        assert!(matches!(writer.write(&[0.0]), Err(IpcError::PartialFrame)));
    }
}
