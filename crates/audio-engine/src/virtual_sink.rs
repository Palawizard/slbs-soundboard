//! Routes the final microphone-plus-soundboard mix to a selectable Windows output
//! device. Pointing it at a loopback cable such as `CABLE Input (VB-Audio Virtual
//! Cable)` exposes the mix to any application as a capture device, without a
//! kernel-mode driver.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_queue::ArrayQueue;

use crate::output::{FrameSource, build_output_stream, find_output_device, output_device_names};
use crate::{CHANNELS, SAMPLE_RATE};

const SINK_CAPACITY_SAMPLES: usize = SAMPLE_RATE as usize * CHANNELS as usize * 2;
const RECOVERY_DELAY: Duration = Duration::from_millis(500);
const IDLE_DELAY: Duration = Duration::from_millis(250);
const DEVICE_WATCH_INTERVAL: Duration = Duration::from_millis(1000);

/// Substrings identifying loopback cables usable as a virtual microphone.
const KNOWN_CABLE_MARKERS: [&str; 4] = ["cable input", "vb-audio", "voicemeeter", "slb virtual"];

/// Reports whether a device name looks like a loopback cable input.
pub fn is_virtual_cable(name: &str) -> bool {
    let lowered = name.to_lowercase();
    KNOWN_CABLE_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

/// Output devices that can carry the virtual microphone, best candidate first.
pub fn virtual_cable_candidates() -> Vec<String> {
    let mut names = output_device_names();
    names.sort_by_key(|name| !is_virtual_cable(name));
    names
}

#[derive(Clone, Debug, Default)]
pub struct VirtualSinkStatus {
    pub enabled: bool,
    pub requested_device: Option<String>,
    pub device_name: Option<String>,
    pub connected: bool,
    pub restart_count: u64,
    pub last_error: Option<String>,
    pub queued_frames: usize,
    pub dropped_frames: u64,
    pub underrun_frames: u64,
}

struct VirtualSinkShared {
    samples: ArrayQueue<f32>,
    enabled: AtomicBool,
    generation: AtomicU64,
    dropped_frames: AtomicU64,
    underrun_frames: AtomicU64,
    device: Mutex<Option<String>>,
    status: Mutex<VirtualSinkStatus>,
}

impl VirtualSinkShared {
    fn new() -> Self {
        Self {
            samples: ArrayQueue::new(SINK_CAPACITY_SAMPLES),
            enabled: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
            underrun_frames: AtomicU64::new(0),
            device: Mutex::new(None),
            status: Mutex::new(VirtualSinkStatus::default()),
        }
    }

    fn requested_device(&self) -> Option<String> {
        self.device
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn active(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
}

/// Real-time side handle. Pushing never allocates and never blocks.
#[derive(Clone)]
pub(crate) struct VirtualSinkProducer {
    shared: Arc<VirtualSinkShared>,
}

impl VirtualSinkProducer {
    pub(crate) fn push(&self, samples: &[f32]) {
        if !self.shared.enabled.load(Ordering::Relaxed) {
            return;
        }
        for frame in samples.chunks_exact(CHANNELS as usize) {
            if self.shared.samples.capacity() - self.shared.samples.len() < CHANNELS as usize {
                self.shared.dropped_frames.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            for sample in frame {
                let _ = self.shared.samples.push(*sample);
            }
        }
    }
}

pub struct VirtualSink {
    shared: Arc<VirtualSinkShared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl VirtualSink {
    pub(crate) fn start() -> std::io::Result<Self> {
        let shared = Arc::new(VirtualSinkShared::new());
        let stop = Arc::new(AtomicBool::new(false));
        let thread_shared = Arc::clone(&shared);
        let thread_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("slb-virtual-sink".to_owned())
            .spawn(move || run_sink(thread_shared, thread_stop))?;
        Ok(Self {
            shared,
            stop,
            thread: Some(worker),
        })
    }

    pub(crate) fn producer(&self) -> VirtualSinkProducer {
        VirtualSinkProducer {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Selects the output device carrying the mix. `None` disables routing.
    pub fn set_device(&self, device: Option<String>) {
        {
            let mut current = self
                .shared
                .device
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *current == device {
                return;
            }
            *current = device.clone();
        }
        self.shared.enabled.store(device.is_some(), Ordering::Release);
        self.shared.generation.fetch_add(1, Ordering::Release);
        self.clear();
    }

    pub fn status(&self) -> VirtualSinkStatus {
        let mut status = self
            .shared
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        status.enabled = self.shared.active();
        status.requested_device = self.shared.requested_device();
        status.queued_frames = self.shared.samples.len() / CHANNELS as usize;
        status.dropped_frames = self.shared.dropped_frames.load(Ordering::Relaxed);
        status.underrun_frames = self.shared.underrun_frames.load(Ordering::Relaxed);
        status
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }

    fn clear(&self) {
        while self.shared.samples.pop().is_some() {}
    }
}

impl Drop for VirtualSink {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_sink(shared: Arc<VirtualSinkShared>, stop: Arc<AtomicBool>) {
    let mut started_once = false;
    while !stop.load(Ordering::Acquire) {
        let generation = shared.generation.load(Ordering::Acquire);
        let Some(requested) = shared.requested_device().filter(|_| shared.active()) else {
            set_disconnected(&shared, None);
            thread::sleep(IDLE_DELAY);
            continue;
        };
        match open_stream(Arc::clone(&shared), &requested) {
            Ok((stream, device_name, errors)) => {
                if started_once {
                    update_status(&shared, |status| status.restart_count += 1);
                }
                started_once = true;
                if let Err(error) = stream.play() {
                    set_disconnected(&shared, Some(error.to_string()));
                    thread::sleep(RECOVERY_DELAY);
                    continue;
                }
                update_status(&shared, |status| {
                    status.device_name = Some(device_name.clone());
                    status.connected = true;
                    status.last_error = None;
                });
                wait_until_invalid(&shared, &stop, generation, &device_name, &errors);
                update_status(&shared, |status| status.connected = false);
            }
            Err(error) => {
                set_disconnected(&shared, Some(error));
                thread::sleep(RECOVERY_DELAY);
            }
        }
    }
    set_disconnected(&shared, None);
}

/// Blocks while the stream stays valid: the device is still present, the selection
/// has not changed and `cpal` has not reported a stream error.
fn wait_until_invalid(
    shared: &Arc<VirtualSinkShared>,
    stop: &Arc<AtomicBool>,
    generation: u64,
    device_name: &str,
    errors: &std::sync::mpsc::Receiver<String>,
) {
    while !stop.load(Ordering::Acquire) {
        if shared.generation.load(Ordering::Acquire) != generation || !shared.active() {
            return;
        }
        if let Ok(error) = errors.recv_timeout(DEVICE_WATCH_INTERVAL) {
            update_status(shared, |status| status.last_error = Some(error));
            return;
        }
        if !output_device_names()
            .iter()
            .any(|name| name.as_str() == device_name)
        {
            update_status(shared, |status| {
                status.last_error =
                    Some("Le périphérique de sortie a été déconnecté.".to_owned())
            });
            return;
        }
    }
}

fn open_stream(
    shared: Arc<VirtualSinkShared>,
    requested: &str,
) -> Result<(cpal::Stream, String, std::sync::mpsc::Receiver<String>), String> {
    let device = find_output_device(requested).ok_or_else(|| {
        format!("Le périphérique « {requested} » est introuvable.")
    })?;
    let device_name = device.name().unwrap_or_else(|_| requested.to_owned());
    let supported = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let sample_format = supported.sample_format();
    let config = supported.config();
    let (error_sender, error_receiver) = std::sync::mpsc::channel();
    let reader = VirtualSinkReader::new(shared, config.sample_rate.0);
    let stream = build_output_stream(&device, &config, sample_format, reader, error_sender)?;
    Ok((stream, device_name, error_receiver))
}

struct VirtualSinkReader {
    shared: Arc<VirtualSinkShared>,
    current: [f32; 2],
    next: [f32; 2],
    phase: f64,
    source_frames_per_output_frame: f64,
    primed: bool,
}

impl VirtualSinkReader {
    fn new(shared: Arc<VirtualSinkShared>, output_rate: u32) -> Self {
        Self {
            shared,
            current: [0.0; 2],
            next: [0.0; 2],
            phase: 0.0,
            source_frames_per_output_frame: SAMPLE_RATE as f64 / output_rate.max(1) as f64,
            primed: false,
        }
    }

    fn pop_frame(&self) -> [f32; 2] {
        match (self.shared.samples.pop(), self.shared.samples.pop()) {
            (Some(left), Some(right)) => [left, right],
            _ => {
                self.shared.underrun_frames.fetch_add(1, Ordering::Relaxed);
                [0.0; 2]
            }
        }
    }
}

impl FrameSource for VirtualSinkReader {
    fn next_frame(&mut self) -> [f32; 2] {
        if !self.shared.enabled.load(Ordering::Relaxed) {
            return [0.0; 2];
        }
        if !self.primed {
            self.current = self.pop_frame();
            self.next = self.pop_frame();
            self.primed = true;
        }
        let phase = self.phase as f32;
        let result = [
            self.current[0] + (self.next[0] - self.current[0]) * phase,
            self.current[1] + (self.next[1] - self.current[1]) * phase,
        ];
        self.phase += self.source_frames_per_output_frame;
        while self.phase >= 1.0 {
            self.current = self.next;
            self.next = self.pop_frame();
            self.phase -= 1.0;
        }
        [result[0].clamp(-1.0, 1.0), result[1].clamp(-1.0, 1.0)]
    }
}

fn set_disconnected(shared: &Arc<VirtualSinkShared>, error: Option<String>) {
    update_status(shared, |status| {
        status.connected = false;
        status.device_name = None;
        if error.is_some() {
            status.last_error = error;
        }
    });
}

fn update_status(shared: &VirtualSinkShared, update: impl FnOnce(&mut VirtualSinkStatus)) {
    let mut status = shared
        .status
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    update(&mut status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cable_names_are_recognized() {
        assert!(is_virtual_cable("CABLE Input (VB-Audio Virtual Cable)"));
        assert!(is_virtual_cable("VoiceMeeter Input (VB-Audio VoiceMeeter VAIO)"));
        assert!(!is_virtual_cable("Haut-parleurs (Realtek(R) Audio)"));
    }

    #[test]
    fn producer_is_bounded_and_disabled_by_default() {
        let shared = Arc::new(VirtualSinkShared::new());
        let producer = VirtualSinkProducer {
            shared: Arc::clone(&shared),
        };
        producer.push(&[0.2, -0.2]);
        assert!(shared.samples.is_empty());
        shared.enabled.store(true, Ordering::Relaxed);
        producer.push(&[0.2, -0.2]);
        assert_eq!(shared.samples.pop(), Some(0.2));
        assert_eq!(shared.samples.pop(), Some(-0.2));
    }

    #[test]
    fn producer_drops_frames_instead_of_blocking_when_full() {
        let shared = Arc::new(VirtualSinkShared::new());
        shared.enabled.store(true, Ordering::Relaxed);
        let producer = VirtualSinkProducer {
            shared: Arc::clone(&shared),
        };
        let frames = vec![0.1_f32; SINK_CAPACITY_SAMPLES + CHANNELS as usize * 16];
        producer.push(&frames);
        assert!(shared.dropped_frames.load(Ordering::Relaxed) > 0);
        assert!(shared.samples.len() <= SINK_CAPACITY_SAMPLES);
    }

    #[test]
    fn reader_passes_the_master_mix_through_without_extra_gain() {
        let shared = Arc::new(VirtualSinkShared::new());
        shared.enabled.store(true, Ordering::Relaxed);
        for sample in [0.5, -0.5, 0.5, -0.5] {
            shared.samples.push(sample).unwrap();
        }
        let mut reader = VirtualSinkReader::new(shared, SAMPLE_RATE);
        assert_eq!(reader.next_frame(), [0.5, -0.5]);
    }

    #[test]
    fn selecting_no_device_disables_routing() {
        let shared = Arc::new(VirtualSinkShared::new());
        let sink = VirtualSink {
            shared: Arc::clone(&shared),
            stop: Arc::new(AtomicBool::new(true)),
            thread: None,
        };
        sink.set_device(Some("CABLE Input (VB-Audio Virtual Cable)".to_owned()));
        assert!(shared.active());
        sink.set_device(None);
        assert!(!shared.active());
    }
}
