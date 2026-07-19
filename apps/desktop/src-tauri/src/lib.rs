use std::sync::Mutex;

use base64::Engine as _;
use serde::Serialize;
use slb_audio_engine::{
    AudioEngine, DeviceCatalog, EngineState, EngineStatus, MixBus, MixerCommand,
    SystemDeviceCatalog,
};
use tauri::{Manager, State};

mod library;

use library::{LibraryService, Sound, Soundboard};

struct AudioAppState {
    engine: Mutex<Option<AudioEngine>>,
}

struct LibraryAppState {
    service: Mutex<LibraryService>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySnapshot {
    soundboards: Vec<Soundboard>,
    recovery_notice: Option<String>,
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
    playback_frames: u64,
    playback_total_frames: u64,
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
            playback_frames: 0,
            playback_total_frames: 0,
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
            playback_frames: status.playback_frames,
            playback_total_frames: status.playback_total_frames,
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

fn with_library<T>(
    state: State<'_, LibraryAppState>,
    action: impl FnOnce(&mut LibraryService) -> Result<T, library::LibraryError>,
) -> Result<T, String> {
    let mut service = state
        .service
        .lock()
        .map_err(|_| "La bibliothèque locale est indisponible.".to_owned())?;
    action(&mut service).map_err(|error| error.to_string())
}

#[tauri::command]
fn library_snapshot(state: State<'_, LibraryAppState>) -> Result<LibrarySnapshot, String> {
    with_library(state, |service| {
        Ok(LibrarySnapshot {
            soundboards: service.soundboards()?,
            recovery_notice: service.recovery_notice(),
        })
    })
}

#[tauri::command]
fn create_soundboard(
    title: String,
    state: State<'_, LibraryAppState>,
) -> Result<Soundboard, String> {
    with_library(state, |service| service.create_soundboard(&title))
}

#[tauri::command]
fn rename_soundboard(
    id: String,
    title: String,
    state: State<'_, LibraryAppState>,
) -> Result<(), String> {
    with_library(state, |service| service.rename_soundboard(&id, &title))
}

#[tauri::command]
fn delete_soundboard(id: String, state: State<'_, LibraryAppState>) -> Result<(), String> {
    with_library(state, |service| service.delete_soundboard(&id))
}

#[tauri::command]
fn reorder_soundboards(ids: Vec<String>, state: State<'_, LibraryAppState>) -> Result<(), String> {
    with_library(state, |service| service.reorder_soundboards(&ids))
}

#[tauri::command]
fn import_sound(
    soundboard_id: String,
    path: String,
    state: State<'_, LibraryAppState>,
) -> Result<Sound, String> {
    with_library(state, |service| {
        service.import_sound(&soundboard_id, std::path::Path::new(&path))
    })
}

#[tauri::command]
fn rename_sound(
    sound_id: String,
    title: String,
    state: State<'_, LibraryAppState>,
) -> Result<(), String> {
    with_library(state, |service| service.rename_sound(&sound_id, &title))
}

#[tauri::command]
fn set_sound_image(
    sound_id: String,
    path: String,
    state: State<'_, LibraryAppState>,
) -> Result<(), String> {
    with_library(state, |service| {
        service.set_sound_image(&sound_id, std::path::Path::new(&path))
    })
}

#[tauri::command]
fn delete_sound(
    soundboard_id: String,
    sound_id: String,
    state: State<'_, LibraryAppState>,
) -> Result<(), String> {
    with_library(state, |service| {
        service.remove_sound(&soundboard_id, &sound_id)
    })
}

#[tauri::command]
fn reorder_sounds(
    soundboard_id: String,
    ids: Vec<String>,
    state: State<'_, LibraryAppState>,
) -> Result<(), String> {
    with_library(state, |service| {
        service.reorder_sounds(&soundboard_id, &ids)
    })
}

#[tauri::command]
fn sound_image_data(hash: String, state: State<'_, LibraryAppState>) -> Result<String, String> {
    with_library(state, |service| {
        let (bytes, extension) = service.image_data(&hash)?;
        let mime = match extension.as_str() {
            "jpg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            _ => "image/png",
        };
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
}

#[tauri::command]
fn play_sound(
    sound_id: String,
    library_state: State<'_, LibraryAppState>,
    audio_state: State<'_, AudioAppState>,
) -> Result<u64, String> {
    let samples = with_library(library_state, |service| {
        service.decode_sound(&sound_id)?.into_engine_samples()
    })?;
    let total_frames = (samples.len() / slb_audio_engine::CHANNELS as usize) as u64;
    let mut slot = audio_state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    if slot.is_none() {
        *slot = Some(AudioEngine::start(None).map_err(|error| error.to_string())?);
    }
    slot.as_ref()
        .expect("audio engine was initialized")
        .play_sound(std::sync::Arc::from(samples.into_boxed_slice()))
        .map_err(|error| error.to_string())?;
    Ok(total_frames)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AudioAppState {
            engine: Mutex::new(None),
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            app.manage(LibraryAppState {
                service: Mutex::new(LibraryService::open(&root)?),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_microphones,
            start_audio,
            stop_audio,
            audio_status,
            play_reference_sound,
            set_microphone_muted,
            library_snapshot,
            create_soundboard,
            rename_soundboard,
            delete_soundboard,
            reorder_soundboards,
            import_sound,
            rename_sound,
            set_sound_image,
            delete_sound,
            reorder_sounds,
            sound_image_data,
            play_sound,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
