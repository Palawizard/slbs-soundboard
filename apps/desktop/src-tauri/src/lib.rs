use std::sync::Mutex;

use serde::Serialize;
use slb_audio_engine::{
    AudioEngine, DeviceCatalog, EngineState, EngineStatus, MixBus, MixerCommand,
    SystemDeviceCatalog,
};
use tauri::State;

struct AudioAppState {
    engine: Mutex<Option<AudioEngine>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MicrophoneDto {
    id: String,
    name: String,
    is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioStatusDto {
    state: &'static str,
    device_id: Option<String>,
    input_sample_rate: Option<u32>,
    input_channels: Option<u16>,
    restart_count: u64,
    last_error: Option<String>,
    peak: f32,
    clipped_samples: u64,
    queued_frames: u64,
    overrun_frames: u64,
    underrun_frames: u64,
}

impl AudioStatusDto {
    fn stopped() -> Self {
        Self {
            state: "stopped",
            device_id: None,
            input_sample_rate: None,
            input_channels: None,
            restart_count: 0,
            last_error: None,
            peak: 0.0,
            clipped_samples: 0,
            queued_frames: 0,
            overrun_frames: 0,
            underrun_frames: 0,
        }
    }
}

impl From<EngineStatus> for AudioStatusDto {
    fn from(status: EngineStatus) -> Self {
        let ipc = status.ipc.unwrap_or(slb_audio_engine::IpcHealth {
            producer_active: false,
            queued_frames: 0,
            overrun_frames: 0,
            underrun_frames: 0,
            consumed_frames: 0,
            heartbeat_ms: 0,
        });
        Self {
            state: match status.state {
                EngineState::Starting => "starting",
                EngineState::Running => "running",
                EngineState::Recovering => "recovering",
                EngineState::Stopped => "stopped",
            },
            device_id: status.device_id,
            input_sample_rate: status.input_sample_rate,
            input_channels: status.input_channels,
            restart_count: status.restart_count,
            last_error: status.last_error,
            peak: status.mix.peak,
            clipped_samples: status.mix.clipped_samples,
            queued_frames: ipc.queued_frames,
            overrun_frames: ipc.overrun_frames,
            underrun_frames: ipc.underrun_frames,
        }
    }
}

#[tauri::command]
fn list_microphones() -> Result<Vec<MicrophoneDto>, String> {
    SystemDeviceCatalog::new()
        .capture_devices()
        .map(|devices| {
            devices
                .into_iter()
                .filter(|device| !device.name.eq_ignore_ascii_case("SLB Virtual Microphone"))
                .map(|device| MicrophoneDto {
                    id: device.id,
                    name: device.name,
                    is_default: device.is_default,
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn start_audio(device_id: Option<String>, state: State<'_, AudioAppState>) -> Result<(), String> {
    let mut slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    if let Some(mut engine) = slot.take() {
        engine.stop().map_err(|error| error.to_string())?;
    }
    *slot = Some(AudioEngine::start(device_id).map_err(|error| error.to_string())?);
    Ok(())
}

#[tauri::command]
fn stop_audio(state: State<'_, AudioAppState>) -> Result<(), String> {
    let engine = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?
        .take();
    if let Some(mut engine) = engine {
        engine.stop().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn audio_status(state: State<'_, AudioAppState>) -> Result<AudioStatusDto, String> {
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    Ok(slot
        .as_ref()
        .map(|engine| engine.status().into())
        .unwrap_or_else(AudioStatusDto::stopped))
}

#[tauri::command]
fn play_reference_sound(state: State<'_, AudioAppState>) -> Result<(), String> {
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    slot.as_ref()
        .ok_or_else(|| "Démarrez d'abord le microphone virtuel.".to_owned())?
        .play_reference_tone()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_microphone_muted(muted: bool, state: State<'_, AudioAppState>) -> Result<(), String> {
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    slot.as_ref()
        .ok_or_else(|| "Démarrez d'abord le microphone virtuel.".to_owned())?
        .send_mixer_command(MixerCommand::SetMuted {
            bus: MixBus::Microphone,
            muted,
        })
        .map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AudioAppState {
            engine: Mutex::new(None),
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_microphones,
            start_audio,
            stop_audio,
            audio_status,
            play_reference_sound,
            set_microphone_muted,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
