use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_queue::ArrayQueue;

use crate::output::{FrameSource, build_output_stream};

use crate::{CHANNELS, SAMPLE_RATE};

const MONITOR_CAPACITY_SAMPLES: usize = SAMPLE_RATE as usize * CHANNELS as usize * 2;
const RECOVERY_DELAY: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Default)]
pub struct MonitorStatus {
    pub enabled: bool,
    pub muted: bool,
    pub gain: f32,
    pub device_name: Option<String>,
    pub restart_count: u64,
    pub last_error: Option<String>,
    pub queued_frames: usize,
    pub dropped_frames: u64,
    pub underrun_frames: u64,
}

struct MonitorShared {
    samples: ArrayQueue<f32>,
    enabled: AtomicBool,
    muted: AtomicBool,
    gain_bits: AtomicU32,
    master_gain_bits: AtomicU32,
    master_muted: AtomicBool,
    dropped_frames: AtomicU64,
    underrun_frames: AtomicU64,
    status: Mutex<MonitorStatus>,
}

impl MonitorShared {
    fn new() -> Self {
        Self {
            samples: ArrayQueue::new(MONITOR_CAPACITY_SAMPLES),
            enabled: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            gain_bits: AtomicU32::new(1.0_f32.to_bits()),
            master_gain_bits: AtomicU32::new(1.0_f32.to_bits()),
            master_muted: AtomicBool::new(false),
            dropped_frames: AtomicU64::new(0),
            underrun_frames: AtomicU64::new(0),
            status: Mutex::new(MonitorStatus::default()),
        }
    }

    fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
            * f32::from_bits(self.master_gain_bits.load(Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub(crate) struct MonitorProducer {
    shared: Arc<MonitorShared>,
}

impl MonitorProducer {
    pub(crate) fn push(&self, samples: &[f32]) {
        if !self.shared.enabled.load(Ordering::Relaxed)
            || self.shared.muted.load(Ordering::Relaxed)
            || self.shared.master_muted.load(Ordering::Relaxed)
        {
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

pub struct MonitorOutput {
    shared: Arc<MonitorShared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MonitorOutput {
    pub(crate) fn start() -> std::io::Result<Self> {
        let shared = Arc::new(MonitorShared::new());
        let stop = Arc::new(AtomicBool::new(false));
        let thread_shared = Arc::clone(&shared);
        let thread_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("slb-output-monitor".to_owned())
            .spawn(move || run_monitor(thread_shared, thread_stop))?;
        Ok(Self {
            shared,
            stop,
            thread: Some(worker),
        })
    }

    pub(crate) fn producer(&self) -> MonitorProducer {
        MonitorProducer {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.shared.enabled.store(enabled, Ordering::Release);
        if !enabled {
            self.clear();
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.shared.muted.store(muted, Ordering::Release);
        if muted {
            self.clear();
        }
    }

    pub fn set_gain(&self, gain: f32) {
        self.shared
            .gain_bits
            .store(gain.clamp(0.0, 2.0).to_bits(), Ordering::Release);
    }

    pub(crate) fn set_master_gain(&self, gain: f32) {
        self.shared
            .master_gain_bits
            .store(gain.clamp(0.0, 2.0).to_bits(), Ordering::Release);
    }

    pub(crate) fn set_master_muted(&self, muted: bool) {
        self.shared.master_muted.store(muted, Ordering::Release);
        if muted {
            self.clear();
        }
    }

    pub fn status(&self) -> MonitorStatus {
        let mut status = self
            .shared
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        status.enabled = self.shared.enabled.load(Ordering::Acquire);
        status.muted = self.shared.muted.load(Ordering::Acquire);
        status.gain = f32::from_bits(self.shared.gain_bits.load(Ordering::Acquire));
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

impl Drop for MonitorOutput {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_monitor(shared: Arc<MonitorShared>, stop: Arc<AtomicBool>) {
    let mut started_once = false;
    while !stop.load(Ordering::Acquire) {
        match open_default_stream(Arc::clone(&shared)) {
            Ok((stream, device_name, errors)) => {
                if started_once {
                    update_status(&shared, |status| status.restart_count += 1);
                }
                started_once = true;
                update_status(&shared, |status| {
                    status.device_name = Some(device_name.clone());
                    status.last_error = None;
                });
                if let Err(error) = stream.play() {
                    update_status(&shared, |status| {
                        status.last_error = Some(error.to_string())
                    });
                    thread::sleep(RECOVERY_DELAY);
                    continue;
                }
                while !stop.load(Ordering::Acquire) {
                    if let Ok(error) = errors.recv_timeout(Duration::from_secs(1)) {
                        update_status(&shared, |status| status.last_error = Some(error));
                        break;
                    }
                    let current_name = cpal::default_host()
                        .default_output_device()
                        .and_then(|device| device.name().ok());
                    if current_name.as_deref() != Some(device_name.as_str()) {
                        break;
                    }
                }
            }
            Err(error) => {
                update_status(&shared, |status| {
                    status.device_name = None;
                    status.last_error = Some(error);
                });
                thread::sleep(RECOVERY_DELAY);
            }
        }
    }
}

fn open_default_stream(
    shared: Arc<MonitorShared>,
) -> Result<(Stream, String, std::sync::mpsc::Receiver<String>), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| "Aucune sortie audio n’est disponible.".to_owned())?;
    let device_name = device.name().unwrap_or_else(|_| "Sortie audio".to_owned());
    let supported = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let sample_format = supported.sample_format();
    let config = supported.config();
    let (error_sender, error_receiver) = std::sync::mpsc::channel();
    let reader = MonitorReader::new(shared, config.sample_rate.0);
    let stream = build_output_stream(&device, &config, sample_format, reader, error_sender)?;
    Ok((stream, device_name, error_receiver))
}

struct MonitorReader {
    shared: Arc<MonitorShared>,
    current: [f32; 2],
    next: [f32; 2],
    phase: f64,
    source_frames_per_output_frame: f64,
    primed: bool,
}

impl MonitorReader {
    fn new(shared: Arc<MonitorShared>, output_rate: u32) -> Self {
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

impl FrameSource for MonitorReader {
    fn next_frame(&mut self) -> [f32; 2] {
        if !self.shared.enabled.load(Ordering::Relaxed)
            || self.shared.muted.load(Ordering::Relaxed)
            || self.shared.master_muted.load(Ordering::Relaxed)
        {
            return [0.0; 2];
        }
        if !self.primed {
            self.current = self.pop_frame();
            self.next = self.pop_frame();
            self.primed = true;
        }
        let phase = self.phase as f32;
        let gain = self.shared.gain();
        let result = [
            (self.current[0] + (self.next[0] - self.current[0]) * phase) * gain,
            (self.current[1] + (self.next[1] - self.current[1]) * phase) * gain,
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

fn update_status(shared: &MonitorShared, update: impl FnOnce(&mut MonitorStatus)) {
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
    fn producer_is_bounded_and_disabled_by_default() {
        let shared = Arc::new(MonitorShared::new());
        let producer = MonitorProducer {
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
    fn reader_converts_channels_and_clamps_gain() {
        let shared = Arc::new(MonitorShared::new());
        shared.enabled.store(true, Ordering::Relaxed);
        shared.gain_bits.store(2.0_f32.to_bits(), Ordering::Relaxed);
        for sample in [0.75, -0.75, 0.75, -0.75] {
            shared.samples.push(sample).unwrap();
        }
        let mut reader = MonitorReader::new(shared, SAMPLE_RATE);
        let frame = reader.next_frame();
        assert_eq!(frame, [1.0, -1.0]);
    }
}
