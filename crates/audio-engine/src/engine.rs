use std::f32::consts::TAU;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::{
    CHANNELS, CaptureError, CaptureSource, DEFAULT_CAPACITY_FRAMES, IpcError, IpcHealth, MixStats,
    RealtimeMixer, SAMPLE_RATE, SharedRingWriter, WasapiCaptureSource, WindowedSincResampler,
    bounded_command_queue,
};
use crate::{CommandReceiver, CommandSender, MixerCommand, QueueFull, ResampleError};

const COMMAND_CAPACITY: usize = 64;
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(10);
const SILENCE_FRAMES: usize = 480;
const STATUS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineState {
    Starting,
    Running,
    Recovering,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct EngineStatus {
    pub state: EngineState,
    pub device_id: Option<String>,
    pub input_sample_rate: Option<u32>,
    pub input_channels: Option<u16>,
    pub restart_count: u64,
    pub last_error: Option<String>,
    pub mix: MixStats,
    pub ipc: Option<IpcHealth>,
    pub playback_frames: u64,
    pub playback_total_frames: u64,
}

impl EngineStatus {
    fn starting(device_id: Option<String>) -> Self {
        Self {
            state: EngineState::Starting,
            device_id,
            input_sample_rate: None,
            input_channels: None,
            restart_count: 0,
            last_error: None,
            mix: MixStats::default(),
            ipc: None,
            playback_frames: 0,
            playback_total_frames: 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error(transparent)]
    Ipc(#[from] IpcError),
    #[error(transparent)]
    Resample(#[from] ResampleError),
    #[error("audio engine command queue is full")]
    CommandQueueFull,
    #[error("audio engine thread failed to start")]
    ThreadStart,
    #[error("audio engine thread panicked")]
    ThreadPanicked,
    #[error("sound buffer must contain complete stereo frames")]
    InvalidSoundBuffer,
}

impl From<QueueFull> for EngineError {
    fn from(_: QueueFull) -> Self {
        Self::CommandQueueFull
    }
}

#[derive(Clone, Debug)]
enum EngineCommand {
    PlayReferenceTone,
    PlaySound(Arc<[f32]>),
    StopSound,
}

pub struct AudioEngine {
    engine_commands: CommandSender<EngineCommand>,
    retired_sounds: CommandReceiver<Arc<[f32]>>,
    mixer_commands: CommandSender<MixerCommand>,
    status: Arc<Mutex<EngineStatus>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioEngine {
    pub fn start(device_id: Option<String>) -> Result<Self, EngineError> {
        let (engine_commands, engine_receiver) = bounded_command_queue(COMMAND_CAPACITY);
        let (retired_sender, retired_sounds) = bounded_command_queue(COMMAND_CAPACITY);
        let (mixer_commands, mixer_receiver) = bounded_command_queue(COMMAND_CAPACITY);
        let status = Arc::new(Mutex::new(EngineStatus::starting(device_id.clone())));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (startup_sender, startup_receiver) = std::sync::mpsc::sync_channel(1);
        let thread_status = Arc::clone(&status);
        let thread_stop = Arc::clone(&stop);

        let audio_thread = thread::Builder::new()
            .name("slb-audio-engine".to_owned())
            .spawn(move || {
                let startup = EngineRuntime::new(
                    device_id,
                    engine_receiver,
                    retired_sender,
                    mixer_receiver,
                    thread_status,
                    thread_stop,
                );
                match startup {
                    Ok(mut runtime) => {
                        let _ = startup_sender.send(Ok(()));
                        runtime.run();
                    }
                    Err(error) => {
                        let _ = startup_sender.send(Err(error));
                    }
                }
            })
            .map_err(|_| EngineError::ThreadStart)?;

        match startup_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                engine_commands,
                retired_sounds,
                mixer_commands,
                status,
                stop,
                thread: Some(audio_thread),
            }),
            Ok(Err(error)) => {
                let _ = audio_thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = audio_thread.join();
                Err(EngineError::ThreadStart)
            }
        }
    }

    pub fn play_reference_tone(&self) -> Result<(), EngineError> {
        self.drain_retired_sounds();
        self.engine_commands
            .try_send(EngineCommand::PlayReferenceTone)?;
        Ok(())
    }

    pub fn play_sound(&self, samples: Arc<[f32]>) -> Result<(), EngineError> {
        if samples.is_empty() || !samples.len().is_multiple_of(CHANNELS as usize) {
            return Err(EngineError::InvalidSoundBuffer);
        }
        self.drain_retired_sounds();
        self.engine_commands
            .try_send(EngineCommand::PlaySound(samples))?;
        Ok(())
    }

    pub fn stop_sound(&self) -> Result<(), EngineError> {
        self.drain_retired_sounds();
        self.engine_commands.try_send(EngineCommand::StopSound)?;
        Ok(())
    }

    pub fn send_mixer_command(&self, command: MixerCommand) -> Result<(), EngineError> {
        self.mixer_commands.try_send(command)?;
        Ok(())
    }

    pub fn status(&self) -> EngineStatus {
        self.drain_retired_sounds();
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn drain_retired_sounds(&self) {
        while self.retired_sounds.try_receive().is_some() {}
    }

    pub fn stop(&mut self) -> Result<(), EngineError> {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(audio_thread) = self.thread.take() {
            audio_thread
                .join()
                .map_err(|_| EngineError::ThreadPanicked)?;
        }
        Ok(())
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

struct CapturePipeline {
    source: WasapiCaptureSource,
    input_channels: usize,
    input: Vec<f32>,
    stereo: Vec<f32>,
    output: Vec<f32>,
    resampler: WindowedSincResampler,
}

impl CapturePipeline {
    fn open(device_id: Option<&str>) -> Result<Self, EngineError> {
        let mut source = WasapiCaptureSource::open(device_id)?;
        let format = source.format();
        let maximum_input_frames = source.maximum_packet_frames().max(1);
        let maximum_output_frames = ((maximum_input_frames as u64 * SAMPLE_RATE as u64)
            .div_ceil(format.sample_rate as u64) as usize)
            + 64;
        let resampler = WindowedSincResampler::new(
            format.sample_rate,
            SAMPLE_RATE,
            CHANNELS as usize,
            maximum_input_frames,
        )?;
        source.start()?;
        Ok(Self {
            source,
            input_channels: format.channels as usize,
            input: vec![0.0; maximum_input_frames * format.channels as usize],
            stereo: vec![0.0; maximum_input_frames * CHANNELS as usize],
            output: vec![0.0; maximum_output_frames * CHANNELS as usize],
            resampler,
        })
    }

    fn format(&self) -> crate::AudioFormat {
        self.source.format()
    }

    fn maximum_output_samples(&self) -> usize {
        self.output.len()
    }

    fn read(&mut self) -> Result<usize, EngineError> {
        let frames = self.source.read_packet(&mut self.input, CAPTURE_TIMEOUT)?;
        if frames == 0 {
            return Ok(0);
        }
        convert_to_stereo(
            &self.input[..frames * self.input_channels],
            self.input_channels,
            &mut self.stereo[..frames * CHANNELS as usize],
        );
        let result = self
            .resampler
            .process(&self.stereo[..frames * CHANNELS as usize], &mut self.output)?;
        Ok(result.output_frames_written)
    }

    fn output(&self, frames: usize) -> &[f32] {
        &self.output[..frames * CHANNELS as usize]
    }
}

struct EngineRuntime {
    device_id: Option<String>,
    pipeline: CapturePipeline,
    writer: SharedRingWriter,
    engine_commands: CommandReceiver<EngineCommand>,
    retired_sounds: CommandSender<Arc<[f32]>>,
    mixer: RealtimeMixer,
    tone: ReferenceTone,
    player: SoundboardPlayer,
    soundboard: Vec<f32>,
    mixed: Vec<f32>,
    silence: [f32; SILENCE_FRAMES * CHANNELS as usize],
    status: Arc<Mutex<EngineStatus>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    reconnect: ReconnectPolicy,
    next_status: Instant,
}

impl EngineRuntime {
    fn new(
        device_id: Option<String>,
        engine_commands: CommandReceiver<EngineCommand>,
        retired_sounds: CommandSender<Arc<[f32]>>,
        mixer_commands: CommandReceiver<MixerCommand>,
        status: Arc<Mutex<EngineStatus>>,
        stop: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, EngineError> {
        let writer = SharedRingWriter::create(DEFAULT_CAPACITY_FRAMES)?;
        let pipeline = CapturePipeline::open(device_id.as_deref())?;
        let sample_capacity = pipeline.maximum_output_samples().max(SILENCE_FRAMES * 2);
        let format = pipeline.format();
        let runtime = Self {
            device_id,
            pipeline,
            writer,
            engine_commands,
            retired_sounds,
            mixer: RealtimeMixer::new(CHANNELS as usize, mixer_commands),
            tone: ReferenceTone::new(),
            player: SoundboardPlayer::new(),
            soundboard: vec![0.0; sample_capacity],
            mixed: vec![0.0; sample_capacity],
            silence: [0.0; SILENCE_FRAMES * CHANNELS as usize],
            status,
            stop,
            reconnect: ReconnectPolicy::new(),
            next_status: Instant::now(),
        };
        runtime.update_state(EngineState::Running, Some(format), None, false);
        Ok(runtime)
    }

    fn run(&mut self) {
        while !self.stop.load(std::sync::atomic::Ordering::Acquire) {
            self.apply_commands();
            match self.pipeline.read() {
                Ok(0) => {
                    let _ = self.writer.write(&[]);
                }
                Ok(frames) => self.process_frames(frames),
                Err(error) => self.recover(error.to_string()),
            }
            self.publish_metrics();
        }
        let _ = self.pipeline.source.stop();
        self.update_state(EngineState::Stopped, None, None, false);
    }

    fn process_frames(&mut self, frames: usize) {
        let samples = frames * CHANNELS as usize;
        self.render_soundboard(samples);
        self.mixer.process(
            self.pipeline.output(frames),
            &self.soundboard[..samples],
            &mut self.mixed[..samples],
        );
        let _ = self.writer.write(&self.mixed[..samples]);
    }

    fn process_recovery_silence(&mut self) {
        let samples = SILENCE_FRAMES * CHANNELS as usize;
        self.render_soundboard(samples);
        self.mixer.process(
            &self.silence,
            &self.soundboard[..samples],
            &mut self.mixed[..samples],
        );
        let _ = self.writer.write(&self.mixed[..samples]);
    }

    fn recover(&mut self, initial_error: String) {
        let mut error = initial_error;
        self.reconnect.failed();
        self.update_state(EngineState::Recovering, None, Some(error.clone()), true);
        while !self.stop.load(std::sync::atomic::Ordering::Acquire) {
            let iteration_start = Instant::now();
            self.apply_commands();
            self.process_recovery_silence();
            if self.reconnect.ready(iteration_start) {
                match CapturePipeline::open(self.device_id.as_deref()) {
                    Ok(pipeline) => {
                        self.pipeline = pipeline;
                        let capacity = self.pipeline.maximum_output_samples();
                        if self.soundboard.len() < capacity {
                            self.soundboard.resize(capacity, 0.0);
                            self.mixed.resize(capacity, 0.0);
                        }
                        self.reconnect.succeeded();
                        self.update_state(
                            EngineState::Running,
                            Some(self.pipeline.format()),
                            None,
                            false,
                        );
                        return;
                    }
                    Err(open_error) => {
                        error = open_error.to_string();
                        self.reconnect.failed();
                        self.update_state(EngineState::Recovering, None, Some(error.clone()), true);
                    }
                }
            }
            let elapsed = iteration_start.elapsed();
            if elapsed < CAPTURE_TIMEOUT {
                thread::sleep(CAPTURE_TIMEOUT - elapsed);
            }
        }
    }

    fn apply_commands(&mut self) {
        while let Some(command) = self.engine_commands.try_receive() {
            match command {
                EngineCommand::PlayReferenceTone => self.tone.trigger(880.0, 500, 0.2),
                EngineCommand::PlaySound(samples) => {
                    if let Some(retired) = self.player.start(samples) {
                        let _ = self.retired_sounds.try_send(retired);
                    }
                }
                EngineCommand::StopSound => {
                    if let Some(retired) = self.player.stop() {
                        let _ = self.retired_sounds.try_send(retired);
                    }
                }
            }
        }
    }

    fn render_soundboard(&mut self, samples: usize) {
        if let Some(retired) = self.player.render(&mut self.soundboard[..samples]) {
            let _ = self.retired_sounds.try_send(retired);
        }
        self.tone.render_additive(&mut self.soundboard[..samples]);
    }

    fn publish_metrics(&mut self) {
        let now = Instant::now();
        if now < self.next_status {
            return;
        }
        self.next_status = now + STATUS_INTERVAL;
        if let Ok(mut status) = self.status.try_lock() {
            status.mix = self.mixer.stats();
            status.ipc = Some(self.writer.health());
            status.playback_frames = self.player.played_frames();
            status.playback_total_frames = self.player.total_frames();
        }
    }

    fn update_state(
        &self,
        state: EngineState,
        format: Option<crate::AudioFormat>,
        error: Option<String>,
        increment_restart: bool,
    ) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.state = state;
        if let Some(format) = format {
            status.input_sample_rate = Some(format.sample_rate);
            status.input_channels = Some(format.channels);
        }
        status.last_error = error;
        if increment_restart {
            status.restart_count += 1;
        }
        status.mix = self.mixer.stats();
        status.ipc = Some(self.writer.health());
        status.playback_frames = self.player.played_frames();
        status.playback_total_frames = self.player.total_frames();
    }
}

struct SoundboardPlayer {
    active: Option<Arc<[f32]>>,
    position: usize,
    last_total_frames: u64,
}

impl SoundboardPlayer {
    const fn new() -> Self {
        Self {
            active: None,
            position: 0,
            last_total_frames: 0,
        }
    }

    fn start(&mut self, samples: Arc<[f32]>) -> Option<Arc<[f32]>> {
        let previous = self.active.replace(samples);
        self.position = 0;
        self.last_total_frames = self
            .active
            .as_ref()
            .map_or(0, |value| (value.len() / CHANNELS as usize) as u64);
        previous
    }

    fn stop(&mut self) -> Option<Arc<[f32]>> {
        self.position = 0;
        self.last_total_frames = 0;
        self.active.take()
    }

    fn render(&mut self, output: &mut [f32]) -> Option<Arc<[f32]>> {
        output.fill(0.0);
        let active = self.active.as_ref()?;
        let remaining = active.len().saturating_sub(self.position);
        let copied = remaining.min(output.len());
        output[..copied].copy_from_slice(&active[self.position..self.position + copied]);
        self.position += copied;
        if self.position >= active.len() {
            self.active.take()
        } else {
            None
        }
    }

    fn played_frames(&self) -> u64 {
        (self.position / CHANNELS as usize) as u64
    }

    const fn total_frames(&self) -> u64 {
        self.last_total_frames
    }
}

struct ReferenceTone {
    phase: f32,
    step: f32,
    level: f32,
    remaining_frames: usize,
}

impl ReferenceTone {
    const fn new() -> Self {
        Self {
            phase: 0.0,
            step: 0.0,
            level: 0.0,
            remaining_frames: 0,
        }
    }

    fn trigger(&mut self, frequency_hz: f32, duration_ms: u32, level: f32) {
        self.phase = 0.0;
        self.step = TAU * frequency_hz.clamp(20.0, 20_000.0) / SAMPLE_RATE as f32;
        self.level = level.clamp(0.0, 1.0);
        self.remaining_frames = SAMPLE_RATE as usize * duration_ms as usize / 1_000;
    }

    fn render_additive(&mut self, output: &mut [f32]) {
        for frame in output.chunks_exact_mut(CHANNELS as usize) {
            if self.remaining_frames == 0 {
                break;
            }
            let sample = self.phase.sin() * self.level;
            for output_sample in frame {
                *output_sample += sample;
            }
            self.phase = (self.phase + self.step) % TAU;
            self.remaining_frames -= 1;
        }
    }
}

struct ReconnectPolicy {
    failures: u32,
    next_attempt: Instant,
}

impl ReconnectPolicy {
    fn new() -> Self {
        Self {
            failures: 0,
            next_attempt: Instant::now(),
        }
    }

    fn failed(&mut self) {
        self.failures = self.failures.saturating_add(1);
        let exponent = self.failures.saturating_sub(1).min(5);
        let delay_ms = (100_u64 << exponent).min(2_000);
        self.next_attempt = Instant::now() + Duration::from_millis(delay_ms);
    }

    fn ready(&self, now: Instant) -> bool {
        now >= self.next_attempt
    }

    fn succeeded(&mut self) {
        self.failures = 0;
        self.next_attempt = Instant::now();
    }
}

fn convert_to_stereo(input: &[f32], input_channels: usize, output: &mut [f32]) {
    debug_assert!(input_channels > 0);
    let frames = input.len() / input_channels;
    debug_assert!(output.len() >= frames * CHANNELS as usize);
    for frame in 0..frames {
        let source = frame * input_channels;
        let destination = frame * CHANNELS as usize;
        output[destination] = input[source];
        output[destination + 1] = if input_channels == 1 {
            input[source]
        } else {
            input[source + 1]
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicates_mono_and_preserves_stereo_channels() {
        let mut mono_output = [0.0; 4];
        convert_to_stereo(&[0.25, -0.5], 1, &mut mono_output);
        assert_eq!(mono_output, [0.25, 0.25, -0.5, -0.5]);

        let mut surround_output = [0.0; 4];
        convert_to_stereo(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 3, &mut surround_output);
        assert_eq!(surround_output, [0.1, 0.2, 0.4, 0.5]);
    }

    #[test]
    fn reference_tone_has_bounded_level_and_duration() {
        let mut tone = ReferenceTone::new();
        tone.trigger(1_000.0, 10, 0.25);
        let mut output = [0.0; 512 * 2];
        tone.render_additive(&mut output);
        assert!(output.iter().all(|sample| sample.abs() <= 0.25));
        assert!(output[480 * 2..].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn reconnect_backoff_is_bounded_and_resets() {
        let mut policy = ReconnectPolicy::new();
        for _ in 0..20 {
            policy.failed();
            assert!(policy.next_attempt <= Instant::now() + Duration::from_millis(2_010));
        }
        policy.succeeded();
        assert_eq!(policy.failures, 0);
        assert!(policy.ready(Instant::now()));
    }

    #[test]
    fn soundboard_player_reports_progress_and_retires_completed_buffer() {
        let mut player = SoundboardPlayer::new();
        player.start(Arc::from(vec![0.25_f32; 8].into_boxed_slice()));
        let mut first = [0.0; 4];
        assert!(player.render(&mut first).is_none());
        assert_eq!(first, [0.25; 4]);
        assert_eq!(player.played_frames(), 2);
        let mut second = [0.0; 6];
        assert!(player.render(&mut second).is_some());
        assert_eq!(&second[..4], &[0.25; 4]);
        assert_eq!(&second[4..], &[0.0; 2]);
        assert_eq!(player.played_frames(), 4);
    }
}
