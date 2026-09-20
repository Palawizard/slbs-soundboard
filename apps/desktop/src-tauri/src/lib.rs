use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use base64::Engine as _;
use serde::Serialize;
use slb_audio_engine::{
    AudioEngine, DeviceCatalog, EngineState, EngineStatus, MixBus, MixerCommand, PlaybackId,
    ReplayPolicy as EngineReplayPolicy, SystemDeviceCatalog, is_virtual_cable, output_device_names,
};
use tauri::{Manager, State};

mod community;
mod driver;
mod library;

use library::{LibraryService, PlaybackProfile, ReplayPolicy, Sound, Soundboard};

struct AudioAppState {
    engine: Mutex<Option<AudioEngine>>,
}

struct LibraryAppState {
    service: Mutex<LibraryService>,
}

struct CommunityAppState {
    client: community::CommunityClient,
}

pub fn run_driver_cli_action(arguments: &[String]) -> Option<i32> {
    driver::run_cli_action(arguments)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySnapshot {
    soundboards: Vec<Soundboard>,
    recovery_notice: Option<String>,
    active_soundboard_id: Option<String>,
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
    playback_paused: bool,
    active_voices: usize,
    monitor_enabled: bool,
    monitor_muted: bool,
    monitor_gain: f32,
    monitor_device_name: Option<String>,
    monitor_restart_count: u64,
    monitor_last_error: Option<String>,
    monitor_queued_frames: usize,
    monitor_dropped_frames: u64,
    monitor_underrun_frames: u64,
    virtual_output_device: Option<String>,
    virtual_output_active_device: Option<String>,
    virtual_output_connected: bool,
    virtual_output_last_error: Option<String>,
    virtual_output_dropped_frames: u64,
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
            playback_paused: false,
            active_voices: 0,
            monitor_enabled: false,
            monitor_muted: false,
            monitor_gain: 1.0,
            monitor_device_name: None,
            monitor_restart_count: 0,
            monitor_last_error: None,
            monitor_queued_frames: 0,
            monitor_dropped_frames: 0,
            monitor_underrun_frames: 0,
            virtual_output_device: None,
            virtual_output_active_device: None,
            virtual_output_connected: false,
            virtual_output_last_error: None,
            virtual_output_dropped_frames: 0,
        }
    }
}

impl From<EngineStatus> for AudioStatusDto {
    fn from(status: EngineStatus) -> Self {
        let monitor = status.monitor;
        let virtual_sink = status.virtual_sink;
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
            playback_paused: status.playback_paused,
            active_voices: status.active_voices,
            monitor_enabled: monitor.enabled,
            monitor_muted: monitor.muted,
            monitor_gain: monitor.gain,
            monitor_device_name: monitor.device_name,
            monitor_restart_count: monitor.restart_count,
            monitor_last_error: monitor.last_error,
            monitor_queued_frames: monitor.queued_frames,
            monitor_dropped_frames: monitor.dropped_frames,
            monitor_underrun_frames: monitor.underrun_frames,
            virtual_output_device: virtual_sink.requested_device,
            virtual_output_active_device: virtual_sink.device_name,
            virtual_output_connected: virtual_sink.connected,
            virtual_output_last_error: virtual_sink.last_error,
            virtual_output_dropped_frames: virtual_sink.dropped_frames,
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
fn start_audio(
    device_id: Option<String>,
    state: State<'_, AudioAppState>,
    library: State<'_, LibraryAppState>,
) -> Result<(), String> {
    let mut slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    if let Some(mut engine) = slot.take() {
        engine.stop().map_err(|error| error.to_string())?;
    }
    let engine = AudioEngine::start(device_id).map_err(|error| error.to_string())?;
    if let Ok(service) = library.service.lock() {
        if let Ok(device) = service.virtual_output_device() {
            engine.set_virtual_output_device(device);
        }
    }
    *slot = Some(engine);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputDeviceDto {
    name: String,
    is_virtual_cable: bool,
}

/// Output devices the mix can be routed to. Loopback cables are listed first
/// because they are the ones usable as a virtual microphone.
#[tauri::command]
fn list_output_devices() -> Vec<OutputDeviceDto> {
    let mut devices: Vec<OutputDeviceDto> = output_device_names()
        .into_iter()
        .map(|name| OutputDeviceDto {
            is_virtual_cable: is_virtual_cable(&name),
            name,
        })
        .collect();
    devices.sort_by(|left, right| {
        right
            .is_virtual_cable
            .cmp(&left.is_virtual_cable)
            .then_with(|| left.name.cmp(&right.name))
    });
    devices
}

/// Opens the VB-Audio download page in the system browser. VB-CABLE is
/// donationware published by VB-Audio; the application only detects it.
#[tauri::command]
fn open_virtual_cable_download(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("https://vb-audio.com/Cable/", None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn virtual_output_device(library: State<'_, LibraryAppState>) -> Result<Option<String>, String> {
    library
        .service
        .lock()
        .map_err(|_| "La bibliotheque est indisponible.".to_owned())?
        .virtual_output_device()
        .map_err(|error| error.to_string())
}

/// Persists the routing choice and applies it immediately when the engine runs.
#[tauri::command]
fn set_virtual_output_device(
    device: Option<String>,
    state: State<'_, AudioAppState>,
    library: State<'_, LibraryAppState>,
) -> Result<(), String> {
    let device = device.filter(|value| !value.trim().is_empty());
    library
        .service
        .lock()
        .map_err(|_| "La bibliotheque est indisponible.".to_owned())?
        .set_virtual_output_device(device.as_deref())
        .map_err(|error| error.to_string())?;
    if let Some(engine) = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?
        .as_ref()
    {
        engine.set_virtual_output_device(device);
    }
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

fn mix_bus(bus: &str) -> Result<MixBus, String> {
    match bus {
        "microphone" => Ok(MixBus::Microphone),
        "soundboard" => Ok(MixBus::Soundboard),
        "master" => Ok(MixBus::Master),
        _ => Err("Ce canal audio n’existe pas.".to_owned()),
    }
}

#[tauri::command]
fn set_master_gain(bus: String, gain: f32, state: State<'_, AudioAppState>) -> Result<(), String> {
    if !gain.is_finite() || !(0.0..=2.0).contains(&gain) {
        return Err("Le niveau demandé n’est pas valide.".to_owned());
    }
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    slot.as_ref()
        .ok_or_else(|| "Démarrez d'abord le microphone virtuel.".to_owned())?
        .send_mixer_command(MixerCommand::SetGain {
            bus: mix_bus(&bus)?,
            gain,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_master_muted(
    bus: String,
    muted: bool,
    state: State<'_, AudioAppState>,
) -> Result<(), String> {
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    slot.as_ref()
        .ok_or_else(|| "Démarrez d'abord le microphone virtuel.".to_owned())?
        .send_mixer_command(MixerCommand::SetMuted {
            bus: mix_bus(&bus)?,
            muted,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_monitoring(enabled: bool, state: State<'_, AudioAppState>) -> Result<(), String> {
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    let engine = slot
        .as_ref()
        .ok_or_else(|| "Démarrez d'abord le microphone virtuel.".to_owned())?;
    engine.set_monitor_enabled(enabled);
    Ok(())
}

#[tauri::command]
fn set_monitor_gain(gain: f32, state: State<'_, AudioAppState>) -> Result<(), String> {
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    slot.as_ref()
        .ok_or_else(|| "Démarrez d'abord le microphone virtuel.".to_owned())?
        .set_monitor_gain(gain)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_monitor_muted(muted: bool, state: State<'_, AudioAppState>) -> Result<(), String> {
    let slot = state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    let engine = slot
        .as_ref()
        .ok_or_else(|| "Démarrez d'abord le microphone virtuel.".to_owned())?;
    engine.set_monitor_muted(muted);
    Ok(())
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
            active_soundboard_id: service.active_soundboard_id()?,
        })
    })
}

#[tauri::command]
fn select_soundboard(id: String, state: State<'_, LibraryAppState>) -> Result<(), String> {
    with_library(state, |service| service.set_active_soundboard(&id))
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
fn update_playback_profile(
    sound_id: String,
    profile: PlaybackProfile,
    state: State<'_, LibraryAppState>,
) -> Result<(), String> {
    with_library(state, |service| {
        service.update_playback_profile(&sound_id, &profile)
    })
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
fn community_api_url(state: State<'_, CommunityAppState>) -> String {
    state.client.base_url()
}

#[tauri::command]
async fn community_login(
    app: tauri::AppHandle,
    state: State<'_, CommunityAppState>,
) -> Result<community::SessionView, String> {
    state.client.login(&app).await
}

#[tauri::command]
async fn community_session(
    state: State<'_, CommunityAppState>,
) -> Result<Option<community::SessionView>, String> {
    state.client.session().await
}

#[tauri::command]
async fn community_update_username(
    username: String,
    state: State<'_, CommunityAppState>,
) -> Result<community::PublicUser, String> {
    state.client.update_username(&username).await
}

#[tauri::command]
async fn community_logout(state: State<'_, CommunityAppState>) -> Result<(), String> {
    state.client.logout().await
}

#[tauri::command]
async fn community_browse(
    query: Option<String>,
    cursor: Option<String>,
    state: State<'_, CommunityAppState>,
) -> Result<community::PublicationList, String> {
    state.client.browse(query, cursor).await
}

#[tauri::command]
async fn community_owned_publications(
    state: State<'_, CommunityAppState>,
    library_state: State<'_, LibraryAppState>,
) -> Result<Vec<community::CommunitySound>, String> {
    let publications = state.client.owned_publications().await?;
    let identities = publications
        .iter()
        .map(|publication| (publication.id.clone(), publication.audio.hash.clone()))
        .collect::<Vec<_>>();
    with_library(library_state, |service| {
        service.reconcile_publications(&identities)
    })?;
    Ok(publications)
}

#[tauri::command]
async fn community_publish_sound(
    sound_id: String,
    description: String,
    progress: tauri::ipc::Channel<community::TransferProgress>,
    library_state: State<'_, LibraryAppState>,
    community_state: State<'_, CommunityAppState>,
) -> Result<community::CommunitySound, String> {
    let (sound, audio, image) = with_library(library_state.clone(), |service| {
        service.sound_assets(&sound_id)
    })?;
    if sound.publication_id.is_some() {
        return Err("Ce son est déjà publié.".to_owned());
    }
    let publication = community_state
        .client
        .publish(
            &audio,
            image.as_deref(),
            &sound.title,
            &description,
            progress,
        )
        .await?;
    let hash = sound.audio_hash.clone();
    let publication_id = publication.id.clone();
    with_library(library_state, |service| {
        service.set_publication(&sound_id, &publication_id, &hash)
    })?;
    Ok(publication)
}

#[tauri::command]
async fn community_delete_publication(
    publication_id: String,
    library_state: State<'_, LibraryAppState>,
    community_state: State<'_, CommunityAppState>,
) -> Result<(), String> {
    community_state
        .client
        .delete_publication(&publication_id)
        .await?;
    with_library(library_state, |service| {
        service.clear_publication(&publication_id)
    })
}

#[tauri::command]
async fn community_import_sound(
    publication_id: String,
    soundboard_id: String,
    progress: tauri::ipc::Channel<community::TransferProgress>,
    app: tauri::AppHandle,
    library_state: State<'_, LibraryAppState>,
    community_state: State<'_, CommunityAppState>,
) -> Result<Sound, String> {
    let staging = app
        .path()
        .app_cache_dir()
        .map_err(|_| "Le dossier temporaire est indisponible.".to_owned())?
        .join("community-staging")
        .join(uuid::Uuid::new_v4().to_string());
    let downloaded = community_state
        .client
        .download_import(&publication_id, &staging, progress)
        .await;
    let result = match downloaded {
        Ok((publication, audio, image)) => with_library(library_state, |service| {
            service.import_community_sound(
                &soundboard_id,
                &audio,
                image.as_deref(),
                &publication.title,
            )
        }),
        Err(error) => Err(error),
    };
    let _ = std::fs::remove_dir_all(&staging);
    if result.is_ok() {
        community_state.client.log(
            "info",
            "community.import",
            "Son importé dans la bibliothèque locale",
        );
    }
    result
}

#[tauri::command]
fn diagnostics_settings(library_state: State<'_, LibraryAppState>) -> Result<bool, String> {
    with_library(library_state, |service| service.diagnostics_enabled())
}

#[tauri::command]
fn set_diagnostics_settings(
    enabled: bool,
    library_state: State<'_, LibraryAppState>,
    community_state: State<'_, CommunityAppState>,
) -> Result<(), String> {
    with_library(library_state, |service| {
        service.set_diagnostics_enabled(enabled)
    })?;
    community_state.client.set_diagnostics(enabled);
    Ok(())
}

#[tauri::command]
fn read_diagnostics(state: State<'_, CommunityAppState>) -> Result<String, String> {
    state.client.read_logs()
}

#[tauri::command]
fn clear_diagnostics(state: State<'_, CommunityAppState>) -> Result<(), String> {
    state.client.clear_logs()
}

#[tauri::command]
fn driver_status(app: tauri::AppHandle) -> driver::DriverStatus {
    driver::status(&app)
}

#[tauri::command]
fn install_driver(app: tauri::AppHandle) -> Result<(), String> {
    driver::request_install(&app)
}

#[tauri::command]
fn remove_driver() -> Result<(), String> {
    driver::request_remove()
}

#[tauri::command]
fn play_sound(
    sound_id: String,
    library_state: State<'_, LibraryAppState>,
    audio_state: State<'_, AudioAppState>,
) -> Result<u64, String> {
    let (sound, samples) = with_library(library_state, |service| service.prepare_sound(&sound_id))?;
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
        .trigger_sound(
            playback_id(&sound.id),
            std::sync::Arc::from(samples.into_boxed_slice()),
            sound.playback.volume,
            match sound.playback.replay_policy {
                ReplayPolicy::Overlap => EngineReplayPolicy::Overlap,
                ReplayPolicy::Toggle => EngineReplayPolicy::Toggle,
                ReplayPolicy::Stop => EngineReplayPolicy::Stop,
                ReplayPolicy::Restart => EngineReplayPolicy::Restart,
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(total_frames)
}

#[tauri::command]
fn stop_all_sounds(audio_state: State<'_, AudioAppState>) -> Result<(), String> {
    let slot = audio_state
        .engine
        .lock()
        .map_err(|_| "Le moteur audio est indisponible.".to_owned())?;
    if let Some(engine) = slot.as_ref() {
        engine
            .stop_all_sounds()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn playback_id(sound_id: &str) -> PlaybackId {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    sound_id.hash(&mut hasher);
    PlaybackId(hasher.finish())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AudioAppState {
            engine: Mutex::new(None),
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            let library = LibraryService::open(&root)?;
            let diagnostics_enabled = library.diagnostics_enabled()?;
            app.manage(LibraryAppState {
                service: Mutex::new(library),
            });
            app.manage(CommunityAppState {
                client: community::CommunityClient::new(&root, diagnostics_enabled)
                    .map_err(std::io::Error::other)?,
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
            set_master_gain,
            set_master_muted,
            set_monitoring,
            list_output_devices,
            virtual_output_device,
            set_virtual_output_device,
            open_virtual_cable_download,
            set_monitor_gain,
            set_monitor_muted,
            library_snapshot,
            select_soundboard,
            create_soundboard,
            rename_soundboard,
            delete_soundboard,
            reorder_soundboards,
            import_sound,
            rename_sound,
            update_playback_profile,
            set_sound_image,
            delete_sound,
            reorder_sounds,
            sound_image_data,
            community_api_url,
            community_login,
            community_session,
            community_update_username,
            community_logout,
            community_browse,
            community_owned_publications,
            community_publish_sound,
            community_delete_publication,
            community_import_sound,
            diagnostics_settings,
            set_diagnostics_settings,
            read_diagnostics,
            clear_diagnostics,
            driver_status,
            install_driver,
            remove_driver,
            play_sound,
            stop_all_sounds,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
