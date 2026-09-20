import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { CSSProperties, FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import "./App.css";
import { LibrarySnapshot, PlaybackProfile, Sound, Soundboard, loadLibrary, moveItem } from "./library";
import { shortcutFromKeyboardEvent, shortcutLabel, useGlobalShortcuts } from "./shortcuts";
import { CommunityPage } from "./CommunityPage";
import { SettingsPage } from "./SettingsPage";
import { communityApi, useCommunityStore } from "./community";
import { communityEnabled } from "./features";
import { UpdateBanner } from "./updates";

type Page = "library" | "audio" | "community" | "settings";
type Microphone = { id: string; name: string; isDefault: boolean };
type AudioStatus = {
  state: "starting" | "running" | "recovering" | "stopped";
  deviceId: string | null;
  inputSampleRate: number | null;
  inputChannels: number | null;
  restartCount: number;
  lastError: string | null;
  peak: number;
  clippedSamples: number;
  queuedFrames: number;
  overrunFrames: number;
  underrunFrames: number;
  playbackFrames: number;
  playbackTotalFrames: number;
  playbackPaused: boolean;
  activeVoices: number;
  monitorEnabled: boolean;
  monitorMuted: boolean;
  monitorGain: number;
  monitorDeviceName: string | null;
  monitorRestartCount: number;
  monitorLastError: string | null;
  monitorQueuedFrames: number;
  monitorDroppedFrames: number;
  monitorUnderrunFrames: number;
  virtualOutputDevice: string | null;
  virtualOutputActiveDevice: string | null;
  virtualOutputConnected: boolean;
  virtualOutputLastError: string | null;
  virtualOutputDroppedFrames: number;
};
type OutputDevice = { name: string; isVirtualCable: boolean };

const stoppedStatus: AudioStatus = {
  state: "stopped", deviceId: null, inputSampleRate: null, inputChannels: null,
  restartCount: 0, lastError: null, peak: 0, clippedSamples: 0, queuedFrames: 0,
  overrunFrames: 0, underrunFrames: 0,
  playbackFrames: 0, playbackTotalFrames: 0,
  playbackPaused: false, activeVoices: 0,
  monitorEnabled: false, monitorMuted: false, monitorGain: 1, monitorDeviceName: null,
  monitorRestartCount: 0, monitorLastError: null, monitorQueuedFrames: 0,
  monitorDroppedFrames: 0, monitorUnderrunFrames: 0,
  virtualOutputDevice: null, virtualOutputActiveDevice: null, virtualOutputConnected: false,
  virtualOutputLastError: null, virtualOutputDroppedFrames: 0,
};
const statusLabels: Record<AudioStatus["state"], string> = {
  starting: "Démarrage", running: "Actif", recovering: "Reconnexion", stopped: "Arrêté",
};

function formatDuration(milliseconds: number) {
  const seconds = Math.max(0, Math.round(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function fallbackColors(seed: string) {
  let hash = 0;
  for (const character of seed) hash = (hash * 31 + character.charCodeAt(0)) | 0;
  const hue = Math.abs(hash) % 360;
  return { background: `linear-gradient(145deg, hsl(${hue} 62% 48%), hsl(${(hue + 48) % 360} 70% 25%))` };
}

function SoundArtwork({ sound }: { sound: Sound }) {
  const [source, setSource] = useState<string | null>(null);
  useEffect(() => {
    if (!sound.imageHash) { setSource(null); return; }
    let active = true;
    void invoke<string>("sound_image_data", { hash: sound.imageHash })
      .then((value) => { if (active) setSource(value); })
      .catch(() => { if (active) setSource(null); });
    return () => { active = false; };
  }, [sound.imageHash]);
  if (source) return <img className="sound-artwork" src={source} alt="" />;
  return <div className="sound-artwork fallback-artwork" style={fallbackColors(sound.audioHash)} aria-hidden="true">{sound.title.slice(0, 2).toUpperCase()}</div>;
}

function Waveform({ peaks, progress }: { peaks: number[]; progress: number }) {
  const values = peaks.length > 0 ? peaks : Array.from({ length: 32 }, (_, index) => 0.18 + ((index * 17) % 7) / 12);
  const bars = values.map((peak, index) => {
    const height = Math.max(2, peak * 18);
    return <rect key={index} x={index * 3} y={(20 - height) / 2} width="1.6" height={height} rx="0.8" />;
  });
  const playedStyle = { width: `${Math.round(progress * 100)}%`, "--progress": Math.max(progress, 0.001) } as CSSProperties;
  return <div className="waveform" aria-hidden="true"><svg viewBox={`0 0 ${values.length * 3} 20`} preserveAspectRatio="none">{bars}</svg><div className="waveform-played" style={playedStyle}><svg viewBox={`0 0 ${values.length * 3} 20`} preserveAspectRatio="none">{bars}</svg></div></div>;
}

type SoundCardProps = {
  sound: Sound;
  index: number;
  total: number;
  busy: boolean;
  playing: boolean;
  progress: number;
  paused: boolean;
  onPlay: (sound: Sound) => void;
  onStop: (sound: Sound) => void;
  onRename: (sound: Sound) => void;
  onImage: (sound: Sound) => void;
  onDelete: (sound: Sound) => void;
  onMove: (from: number, to: number) => void;
  onProfile: (sound: Sound, profile: PlaybackProfile) => void;
  shortcutOwner: (shortcut: string, soundId: string) => string | null;
};

const replayLabels = { overlap: "Superposer", toggle: "Pause ou reprendre", stop: "Arrêter", restart: "Recommencer" } as const;

function SoundCard({ sound, index, total, busy, playing, progress, paused, onPlay, onStop, onRename, onImage, onDelete, onMove, onProfile, shortcutOwner }: SoundCardProps) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [capturingShortcut, setCapturingShortcut] = useState(false);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [draft, setDraft] = useState(sound.playback);
  useEffect(() => setDraft(sound.playback), [sound.playback]);
  useEffect(() => {
    if (!settingsOpen || draft === sound.playback) return;
    const timer = window.setTimeout(() => onProfile(sound, draft), 400);
    return () => window.clearTimeout(timer);
  }, [draft, settingsOpen, sound, onProfile]);
  const updateDraft = (next: Partial<PlaybackProfile>) => setDraft((current) => ({ ...current, ...next }));
  const captureShortcut = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    if (!capturingShortcut) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.code === "Escape") { setCapturingShortcut(false); setShortcutError(null); return; }
    if (event.code === "Backspace" || event.code === "Delete") {
      updateDraft({ keybind: null }); setCapturingShortcut(false); setShortcutError(null); return;
    }
    const shortcut = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!shortcut) { setShortcutError("Utilisez au moins Ctrl, Alt, Maj ou Windows avec une touche prise en charge."); return; }
    const owner = shortcutOwner(shortcut, sound.id);
    if (owner) { setShortcutError(`Ce raccourci est déjà utilisé par « ${owner} ».`); return; }
    updateDraft({ keybind: shortcut });
    setCapturingShortcut(false);
    setShortcutError(null);
  };
  return (
    <article className="sound-card">
      <button className="sound-trigger" type="button" onClick={() => onPlay(sound)} disabled={busy} aria-label={`Jouer ${sound.title}`}>
        <SoundArtwork sound={sound} />
        <span className="play-mark" aria-hidden="true">{playing && paused ? "Ⅱ" : "▶"}</span>
      </button>
      <div className="sound-copy">
        <strong title={sound.title}>{sound.title}</strong>
        <span>{formatDuration(sound.durationMs)}</span>
      </div>
      <Waveform peaks={sound.waveform} progress={playing ? progress : 0} />
      <div className="card-actions" aria-label={`Actions pour ${sound.title}`}>
        <button type="button" onClick={() => onMove(index, index - 1)} disabled={busy || index === 0} title="Déplacer avant">←</button>
        <button type="button" onClick={() => onMove(index, index + 1)} disabled={busy || index === total - 1} title="Déplacer après">→</button>
        <button type="button" onClick={() => onStop(sound)} disabled={!playing} title="Arrêter ce son">Arrêter</button>
        <button type="button" onClick={() => onImage(sound)} disabled={busy} title="Choisir une image">Image</button>
        <button type="button" onClick={() => onRename(sound)} disabled={busy} title="Renommer">Renommer</button>
        <button type="button" aria-expanded={settingsOpen} onClick={() => setSettingsOpen((value) => !value)} disabled={busy}>Réglages</button>
        <button className="delete-action" type="button" onClick={() => onDelete(sound)} disabled={busy} title="Supprimer">Supprimer</button>
      </div>
      {settingsOpen && <div className="sound-settings" aria-label={`Réglages de ${sound.title}`}>
        <label>Volume <output>{Math.round(draft.volume * 100)} %</output><input type="range" min="0" max="2" step="0.05" value={draft.volume} onChange={(event) => updateDraft({ volume: Number(event.target.value) })} /></label>
        <label>Hauteur <output>{draft.pitchSemitones > 0 ? "+" : ""}{draft.pitchSemitones} demi-ton{Math.abs(draft.pitchSemitones) > 1 ? "s" : ""}</output><input type="range" min="-12" max="12" step="1" value={draft.pitchSemitones} onChange={(event) => updateDraft({ pitchSemitones: Number(event.target.value) })} /></label>
        <label>Vitesse <output>{draft.speed.toFixed(2)}×</output><input type="range" min="0.5" max="2" step="0.05" value={draft.speed} onChange={(event) => updateDraft({ speed: Number(event.target.value) })} /></label>
        <label>Au second appui<select value={draft.replayPolicy} onChange={(event) => updateDraft({ replayPolicy: event.target.value as PlaybackProfile["replayPolicy"] })}>{Object.entries(replayLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <div className="shortcut-setting"><span>Raccourci global</span><kbd>{capturingShortcut ? "Appuyez sur les touches…" : shortcutLabel(draft.keybind)}</kbd><div><button type="button" className="shortcut-capture" onClick={() => { setCapturingShortcut(true); setShortcutError(null); }} onKeyDown={captureShortcut}>{capturingShortcut ? "Écoute…" : "Définir"}</button>{draft.keybind && <button type="button" onClick={() => updateDraft({ keybind: null })}>Effacer</button>}</div></div>
        {shortcutError && <p className="settings-error" role="alert">{shortcutError}</p>}
        <p className="settings-hint" role="status">Les changements sont enregistrés automatiquement. Utilisez le bouton de lecture pour écouter le résultat.</p>
      </div>}
    </article>
  );
}

function LibraryPage() {
  const [snapshot, setSnapshot] = useState<LibrarySnapshot>({ soundboards: [], recoveryNotice: null, activeSoundboardId: null });
  const [selectedId, setSelectedId] = useState("");
  const [newTitle, setNewTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [playing, setPlaying] = useState<{ id: string; totalFrames: number; progress: number; paused: boolean } | null>(null);
  const reportShortcutError = useCallback((message: string) => setError(message), []);
  useGlobalShortcuts(snapshot.soundboards, reportShortcutError);

  const refresh = useCallback(async (preferredId?: string) => {
    const next = await loadLibrary();
    setSnapshot(next);
    setNotice(next.recoveryNotice);
    setSelectedId((current) => {
      const preferred = preferredId ?? (current || next.activeSoundboardId || "");
      return next.soundboards.some((board) => board.id === preferred) ? preferred : (next.soundboards[0]?.id ?? "");
    });
  }, []);

  useEffect(() => { void refresh().catch((reason: unknown) => setError(String(reason))); }, [refresh]);
  useEffect(() => {
    if (!playing) return;
    const timer = window.setInterval(() => {
      void invoke<AudioStatus>("audio_status").then((status) => {
        if (status.playbackTotalFrames === 0) { setPlaying(null); return; }
        if (status.playbackTotalFrames !== playing.totalFrames) return;
        const progress = Math.min(1, status.playbackFrames / Math.max(1, playing.totalFrames));
        if (progress >= 1) setPlaying(null);
        else setPlaying((current) => current?.id === playing.id ? { ...current, progress, paused: status.playbackPaused } : current);
      }).catch((reason: unknown) => { setError(String(reason)); setPlaying(null); });
    }, 120);
    return () => window.clearInterval(timer);
  }, [playing?.id, playing?.totalFrames]);
  const selected = useMemo(() => snapshot.soundboards.find((board) => board.id === selectedId) ?? snapshot.soundboards[0], [snapshot.soundboards, selectedId]);

  async function run(action: () => Promise<unknown>, success?: string, preferredId?: string) {
    setBusy(true); setError(null); setNotice(null);
    try { await action(); await refresh(preferredId); if (success) setNotice(success); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function createBoard(event: FormEvent) {
    event.preventDefault();
    const title = newTitle.trim(); if (!title) return;
    setNewTitle("");
    setBusy(true); setError(null); setNotice(null);
    try {
      const board = await invoke<Soundboard>("create_soundboard", { title });
      await invoke("select_soundboard", { id: board.id });
      await refresh(board.id);
      setNotice("Soundboard créé.");
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function renameBoard(board: Soundboard) {
    const title = window.prompt("Nouveau nom", board.title)?.trim();
    if (title && title !== board.title) await run(() => invoke("rename_soundboard", { id: board.id, title }), "Soundboard renommé.", board.id);
  }

  async function deleteBoard(board: Soundboard) {
    if (!window.confirm(`Supprimer « ${board.title} » ? Les sons uniquement présents ici seront aussi supprimés.`)) return;
    await run(() => invoke("delete_soundboard", { id: board.id }), "Soundboard supprimé.");
  }

  async function moveBoard(from: number, to: number) {
    const reordered = moveItem(snapshot.soundboards, from, to); if (reordered === snapshot.soundboards) return;
    setSnapshot((current) => ({ ...current, soundboards: reordered }));
    await run(() => invoke("reorder_soundboards", { ids: reordered.map((board) => board.id) }), undefined, selectedId);
  }

  async function importSounds() {
    if (!selected) return;
    const paths = await open({ multiple: true, filters: [{ name: "Sons", extensions: ["wav", "mp3", "flac", "ogg", "m4a", "aac"] }] });
    if (!paths) return;
    const files = Array.isArray(paths) ? paths : [paths];
    await run(async () => { for (const path of files) await invoke("import_sound", { soundboardId: selected.id, path }); }, files.length > 1 ? `${files.length} sons ajoutés.` : "Son ajouté.", selected.id);
  }

  async function renameSound(sound: Sound) {
    const title = window.prompt("Nouveau nom", sound.title)?.trim();
    if (title && title !== sound.title) await run(() => invoke("rename_sound", { soundId: sound.id, title }), "Son renommé.", selected?.id);
  }

  async function chooseImage(sound: Sound) {
    const path = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] }] });
    if (typeof path === "string") await run(() => invoke("set_sound_image", { soundId: sound.id, path }), "Image mise à jour.", selected?.id);
  }

  async function deleteSound(sound: Sound) {
    if (!selected || !window.confirm(`Supprimer « ${sound.title} » de ce soundboard ?`)) return;
    await run(() => invoke("delete_sound", { soundboardId: selected.id, soundId: sound.id }), "Son supprimé.", selected.id);
  }

  async function moveSound(from: number, to: number) {
    if (!selected) return;
    const sounds = moveItem(selected.sounds, from, to); if (sounds === selected.sounds) return;
    setSnapshot((current) => ({ ...current, soundboards: current.soundboards.map((board) => board.id === selected.id ? { ...board, sounds } : board) }));
    await run(() => invoke("reorder_sounds", { soundboardId: selected.id, ids: sounds.map((sound) => sound.id) }), undefined, selected.id);
  }

  async function playSound(sound: Sound) {
    setError(null);
    try {
      const totalFrames = await invoke<number>("play_sound", { soundId: sound.id });
      setPlaying({ id: sound.id, totalFrames, progress: 0, paused: false });
    } catch (reason) {
      setError(String(reason));
    }
  }

  const updateProfile = useCallback(async (sound: Sound, profile: PlaybackProfile) => {
    setError(null);
    try {
      await invoke("update_playback_profile", { soundId: sound.id, profile });
      setSnapshot((current) => ({ ...current, soundboards: current.soundboards.map((board) => ({ ...board, sounds: board.sounds.map((item) => item.id === sound.id ? { ...item, playback: profile } : item) })) }));
    } catch (reason) { setError(String(reason)); }
  }, []);

  const shortcutOwner = useCallback((shortcut: string, soundId: string) => {
    const owner = snapshot.soundboards.flatMap((board) => board.sounds)
      .find((sound) => sound.id !== soundId && sound.playback.keybind?.toLowerCase() === shortcut.toLowerCase());
    return owner?.title ?? null;
  }, [snapshot.soundboards]);

  async function stopSound(sound: Sound) {
    try { await invoke("stop_sound", { soundId: sound.id }); setPlaying((current) => current?.id === sound.id ? null : current); }
    catch (reason) { setError(String(reason)); }
  }

  async function stopPlayback() {
    try { await invoke("stop_all_sounds"); setPlaying(null); }
    catch (reason) { setError(String(reason)); }
  }

  return (
    <div className="library-layout">
      <aside className="board-rail" aria-label="Soundboards">
        <div className="rail-heading"><div><p className="eyebrow">Bibliothèque</p><h2>Mes soundboards</h2></div></div>
        <form className="new-board" onSubmit={createBoard}>
          <label className="sr-only" htmlFor="new-board-title">Nom du soundboard</label>
          <input id="new-board-title" value={newTitle} maxLength={80} onChange={(event) => setNewTitle(event.target.value)} placeholder="Nouveau soundboard" disabled={busy} />
          <button type="submit" disabled={busy || !newTitle.trim()} aria-label="Créer le soundboard">+</button>
        </form>
        <div className="board-list">
          {snapshot.soundboards.map((board, index) => (
            <div className={`board-row ${selected?.id === board.id ? "selected" : ""}`} key={board.id}>
              <button className="board-select" type="button" onClick={() => { setSelectedId(board.id); void invoke("select_soundboard", { id: board.id }).catch((reason: unknown) => setError(String(reason))); }}><strong>{board.title}</strong><span>{board.sounds.length} son{board.sounds.length > 1 ? "s" : ""}</span></button>
              <div className="board-actions">
                <button type="button" onClick={() => moveBoard(index, index - 1)} disabled={busy || index === 0} title="Monter">↑</button>
                <button type="button" onClick={() => moveBoard(index, index + 1)} disabled={busy || index === snapshot.soundboards.length - 1} title="Descendre">↓</button>
                <button type="button" onClick={() => renameBoard(board)} disabled={busy} title="Renommer">✎</button>
                <button type="button" onClick={() => deleteBoard(board)} disabled={busy} title="Supprimer">×</button>
              </div>
            </div>
          ))}
        </div>
      </aside>
      <section className="library-content">
        <header className="library-header">
          <div><p className="eyebrow">Soundboard</p><h1>{selected?.title ?? "Mes sons"}</h1><p>{selected?.sounds.length ?? 0} son{selected?.sounds.length === 1 ? "" : "s"}</p></div>
          <div className="header-actions"><button className="secondary-button" type="button" onClick={stopPlayback} disabled={!playing}>Tout arrêter</button><button className="primary-button" type="button" onClick={importSounds} disabled={!selected || busy}>{busy ? "Patientez…" : "Ajouter des sons"}</button></div>
        </header>
        {notice && <p className="notice-message" role="status">{notice}</p>}
        {error && <p className="error-message" role="alert">{error}</p>}
        {selected && selected.sounds.length > 0 ? (
          <div className="sound-grid">
            {selected.sounds.map((sound, index) => <SoundCard key={sound.id} sound={sound} index={index} total={selected.sounds.length} busy={busy} playing={playing?.id === sound.id} progress={playing?.id === sound.id ? playing.progress : 0} paused={playing?.id === sound.id ? playing.paused : false} onPlay={playSound} onStop={stopSound} onRename={renameSound} onImage={chooseImage} onDelete={deleteSound} onMove={moveSound} onProfile={updateProfile} shortcutOwner={shortcutOwner} />)}
          </div>
        ) : (
          <div className="empty-state"><div className="empty-icon" aria-hidden="true">♪</div><h2>Votre soundboard est vide</h2><p>Ajoutez vos premiers sons pour les retrouver ici.</p><button className="primary-button" type="button" onClick={importSounds} disabled={!selected || busy}>Choisir des sons</button></div>
        )}
      </section>
    </div>
  );
}

type MixControlBus = "microphone" | "soundboard" | "master";

function AudioPage() {
  const [microphones, setMicrophones] = useState<Microphone[]>([]); const [selectedId, setSelectedId] = useState("");
  const [status, setStatus] = useState<AudioStatus>(stoppedStatus); const [busy, setBusy] = useState(false);
  const [levels, setLevels] = useState<Record<MixControlBus, number>>({ microphone: 1, soundboard: 1, master: 1 });
  const [mutes, setMutes] = useState<Record<MixControlBus, boolean>>({ microphone: false, soundboard: false, master: false });
  const [monitorEnabled, setMonitorEnabled] = useState(true); const [monitorMuted, setMonitorMuted] = useState(false); const [monitorGain, setMonitorGain] = useState(1);
  const [outputs, setOutputs] = useState<OutputDevice[]>([]); const [virtualOutput, setVirtualOutput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const isRunning = status.state === "running" || status.state === "recovering";
  const selectedMicrophone = microphones.find((microphone) => microphone.id === selectedId);
  const hasCable = outputs.some((device) => device.isVirtualCable);
  useEffect(() => { void invoke<Microphone[]>("list_microphones").then((devices) => { setMicrophones(devices); setSelectedId((current) => current || ((devices.find((device) => device.isDefault) ?? devices[0])?.id ?? "")); }).catch((reason: unknown) => setError(String(reason))); }, []);
  useEffect(() => { if (status.deviceId) setSelectedId(status.deviceId); }, [status.deviceId]);
  const refreshOutputs = useCallback(async () => {
    try {
      const devices = await invoke<OutputDevice[]>("list_output_devices");
      setOutputs(devices);
      const stored = await invoke<string | null>("virtual_output_device");
      setVirtualOutput(stored && devices.some((device) => device.name === stored) ? stored : stored ?? "");
    } catch (reason) { setError(String(reason)); }
  }, []);
  useEffect(() => { void refreshOutputs(); }, [refreshOutputs]);
  async function changeVirtualOutput(name: string) {
    setVirtualOutput(name);
    try { await invoke("set_virtual_output_device", { device: name || null }); }
    catch (reason) { setError(String(reason)); }
  }
  useEffect(() => { const refresh = () => void invoke<AudioStatus>("audio_status").then(setStatus).catch((reason: unknown) => setError(String(reason))); refresh(); const timer = window.setInterval(refresh, 500); return () => window.clearInterval(timer); }, []);
  // The engine is started by the application itself; this only switches device.
  async function changeMicrophone(deviceId: string) {
    setSelectedId(deviceId); setBusy(true); setError(null);
    try {
      await invoke("start_audio", { deviceId: deviceId || null });
      for (const bus of ["microphone", "soundboard", "master"] as const) {
        await invoke("set_master_gain", { bus, gain: levels[bus] });
        await invoke("set_master_muted", { bus, muted: mutes[bus] });
      }
      await invoke("set_monitor_gain", { gain: monitorGain });
      await invoke("set_monitor_muted", { muted: monitorMuted });
      await invoke("set_monitoring", { enabled: monitorEnabled });
      setStatus(await invoke<AudioStatus>("audio_status"));
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }
  async function changeLevel(bus: MixControlBus, gain: number) { setLevels((current) => ({ ...current, [bus]: gain })); if (isRunning) try { await invoke("set_master_gain", { bus, gain }); } catch (reason) { setError(String(reason)); } }
  async function toggleBusMute(bus: MixControlBus) { const muted = !mutes[bus]; setMutes((current) => ({ ...current, [bus]: muted })); if (isRunning) try { await invoke("set_master_muted", { bus, muted }); } catch (reason) { setError(String(reason)); } }
  async function toggleMonitoring() { const enabled = !monitorEnabled; try { await invoke("set_monitoring", { enabled }); setMonitorEnabled(enabled); } catch (reason) { setError(String(reason)); } }
  async function changeMonitorGain(gain: number) { setMonitorGain(gain); if (isRunning) try { await invoke("set_monitor_gain", { gain }); } catch (reason) { setError(String(reason)); } }
  async function toggleMonitorMute() { const muted = !monitorMuted; try { await invoke("set_monitor_muted", { muted }); setMonitorMuted(muted); } catch (reason) { setError(String(reason)); } }
  return (
    <section className="workspace"><header className="topbar"><div><p className="eyebrow">Chemin audio</p><h1>Microphone virtuel</h1></div><span className={`status status-${status.state}`}><span className="status-dot" />{statusLabels[status.state]}</span></header>
      <section className="audio-panel" aria-labelledby="audio-title"><div className="panel-copy"><p className="eyebrow">Source physique</p><h2 id="audio-title">Choisissez votre microphone</h2><p>Votre micro est actif dès l’ouverture de l’application. Votre voix et les sons sont réunis, puis envoyés vers le câble virtuel que vous choisissez ci-dessous.</p></div>
        <label className="field-label" htmlFor="microphone-select">Microphone d’entrée</label><select id="microphone-select" value={selectedId} onChange={(event) => void changeMicrophone(event.target.value)} disabled={busy}>{microphones.length === 0 && <option value="">Aucun microphone détecté</option>}{microphones.map((microphone) => <option key={microphone.id} value={microphone.id}>{microphone.name}{microphone.isDefault ? " — par défaut" : ""}</option>)}</select>
        <div className="actions">{!isRunning && <button className="primary-button" type="button" onClick={() => void changeMicrophone(selectedId)} disabled={busy}>{busy ? "Connexion…" : "Réessayer"}</button>}<button className="secondary-button" type="button" onClick={() => void invoke("play_reference_sound").catch((reason: unknown) => setError(String(reason)))} disabled={!isRunning}>Jouer le son test</button></div>
        <div className="signal-card"><div><span className="signal-label">Entrée</span><strong>{selectedMicrophone?.name ?? "Non sélectionnée"}</strong></div><div className="route-line"><span /></div><div><span className="signal-label">Sortie</span><strong>{virtualOutput || "Aucune sortie choisie"}</strong></div></div>
        <section className="routing-panel" aria-labelledby="routing-title">
          <div className="panel-copy"><p className="eyebrow">Destination</p><h2 id="routing-title">Sortie vers le micro virtuel</h2></div>
          <label className="field-label" htmlFor="virtual-output-select">Câble virtuel</label>
          <select id="virtual-output-select" value={virtualOutput} onChange={(event) => void changeVirtualOutput(event.target.value)} disabled={busy}>
            <option value="">Aucune sortie</option>
            {outputs.map((device) => <option key={device.name} value={device.name}>{device.name}{device.isVirtualCable ? " — câble virtuel" : ""}</option>)}
          </select>
          {!hasCable
            ? <p className="hint">Aucun câble virtuel détecté. Installez VB-CABLE, un logiciel gratuit (donationware) publié par VB-Audio, puis actualisez la liste.
                <button type="button" className="link-button" onClick={() => void invoke("open_virtual_cable_download").catch((reason: unknown) => setError(String(reason)))}>Télécharger VB-CABLE</button></p>
            : virtualOutput
              ? <p className="hint">Dans Discord ou votre jeu, choisissez <strong>{virtualOutput.replace("Input", "Output")}</strong> comme microphone.</p>
              : <p className="hint">Choisissez le câble ci-dessus pour que vos sons sortent dans vos appels.</p>}
          <div className="routing-state"><span className={`status status-${status.virtualOutputConnected ? "running" : "stopped"}`}><span className="status-dot" />{status.virtualOutputConnected ? `Connecté à ${status.virtualOutputActiveDevice}` : "Non connecté"}</span>
            <button type="button" className="secondary-button" onClick={() => void refreshOutputs()}>Actualiser la liste</button></div>
        </section>
        <section className="mix-controls" aria-labelledby="mix-controls-title"><div className="mix-heading"><div><p className="eyebrow">Mixage</p><h2 id="mix-controls-title">Niveaux maîtres</h2></div><span>{Math.round(status.peak * 100)} %</span></div>
          {(["microphone", "soundboard", "master"] as const).map((bus) => <div className="mix-row" key={bus}><label htmlFor={`gain-${bus}`}>{bus === "microphone" ? "Microphone" : bus === "soundboard" ? "Sons" : "Sortie virtuelle"}</label><input id={`gain-${bus}`} type="range" min="0" max="2" step="0.05" value={levels[bus]} onChange={(event) => void changeLevel(bus, Number(event.target.value))} /><output>{Math.round(levels[bus] * 100)} %</output><button type="button" onClick={() => void toggleBusMute(bus)} disabled={!isRunning}>{mutes[bus] ? "Réactiver" : "Couper"}</button></div>)}
          <div className="monitor-control"><div><strong>Écoute des sons dans vos écouteurs</strong><span>{status.monitorDeviceName ?? "Sortie par défaut"}{status.monitorRestartCount > 0 ? ` · ${status.monitorRestartCount} reconnexion${status.monitorRestartCount > 1 ? "s" : ""}` : ""}</span></div><button className="secondary-button" type="button" onClick={toggleMonitoring} disabled={!isRunning}>{monitorEnabled ? "Désactiver" : "Activer"}</button></div>
          <div className="mix-row"><label htmlFor="monitor-gain">Volume d’écoute</label><input id="monitor-gain" type="range" min="0" max="2" step="0.05" value={monitorGain} onChange={(event) => void changeMonitorGain(Number(event.target.value))} /><output>{Math.round(monitorGain * 100)} %</output><button type="button" onClick={toggleMonitorMute} disabled={!isRunning || !monitorEnabled}>{monitorMuted ? "Réactiver" : "Couper"}</button></div>
        </section>
        {isRunning && <dl className="metrics"><div><dt>Format</dt><dd>{status.inputSampleRate ? `${status.inputSampleRate / 1000} kHz` : "—"}</dd></div><div><dt>Écrêtages</dt><dd>{status.clippedSamples}</dd></div><div><dt>File virtuelle</dt><dd>{status.queuedFrames} trames</dd></div><div><dt>Reconnexions</dt><dd>{status.restartCount}</dd></div></dl>}{(error || status.lastError || status.virtualOutputLastError || (monitorEnabled && status.monitorLastError)) && <p className="error-message" role="alert">{error ?? status.lastError ?? status.virtualOutputLastError ?? status.monitorLastError}</p>}
      </section></section>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("library");
  const setSession = useCommunityStore((state) => state.setSession);
  useEffect(() => { if (communityEnabled) void communityApi.session().then(setSession).catch(() => setSession(null)); }, [setSession]);
  return <main className="app-shell"><aside className="sidebar"><div className="brand-mark">SLB</div><nav aria-label="Navigation principale"><button className={`nav-item ${page === "library" ? "active" : ""}`} type="button" onClick={() => setPage("library")}>Mes soundboards</button><button className={`nav-item ${page === "audio" ? "active" : ""}`} type="button" onClick={() => setPage("audio")}>Audio</button>{communityEnabled && <button className={`nav-item ${page === "community" ? "active" : ""}`} type="button" onClick={() => setPage("community")}>Communauté</button>}<button className={`nav-item ${page === "settings" ? "active" : ""}`} type="button" onClick={() => setPage("settings")}>Réglages</button></nav></aside><UpdateBanner />{page === "library" ? <LibraryPage /> : page === "audio" ? <AudioPage /> : page === "community" && communityEnabled ? <CommunityPage /> : <SettingsPage />}</main>;
}
