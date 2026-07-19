import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { CSSProperties, FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import "./App.css";
import { LibrarySnapshot, Sound, Soundboard, loadLibrary, moveItem } from "./library";

type Page = "library" | "audio";
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
};

const stoppedStatus: AudioStatus = {
  state: "stopped", deviceId: null, inputSampleRate: null, inputChannels: null,
  restartCount: 0, lastError: null, peak: 0, clippedSamples: 0, queuedFrames: 0,
  overrunFrames: 0, underrunFrames: 0,
  playbackFrames: 0, playbackTotalFrames: 0,
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
  onPlay: (sound: Sound) => void;
  onRename: (sound: Sound) => void;
  onImage: (sound: Sound) => void;
  onDelete: (sound: Sound) => void;
  onMove: (from: number, to: number) => void;
};

function SoundCard({ sound, index, total, busy, playing, progress, onPlay, onRename, onImage, onDelete, onMove }: SoundCardProps) {
  return (
    <article className="sound-card">
      <button className="sound-trigger" type="button" onClick={() => onPlay(sound)} disabled={busy} aria-label={`Jouer ${sound.title}`}>
        <SoundArtwork sound={sound} />
        <span className="play-mark" aria-hidden="true">▶</span>
      </button>
      <div className="sound-copy">
        <strong title={sound.title}>{sound.title}</strong>
        <span>{formatDuration(sound.durationMs)}</span>
      </div>
      <Waveform peaks={sound.waveform} progress={playing ? progress : 0} />
      <div className="card-actions" aria-label={`Actions pour ${sound.title}`}>
        <button type="button" onClick={() => onMove(index, index - 1)} disabled={busy || index === 0} title="Déplacer avant">←</button>
        <button type="button" onClick={() => onMove(index, index + 1)} disabled={busy || index === total - 1} title="Déplacer après">→</button>
        <button type="button" onClick={() => onImage(sound)} disabled={busy} title="Choisir une image">Image</button>
        <button type="button" onClick={() => onRename(sound)} disabled={busy} title="Renommer">Renommer</button>
        <button className="delete-action" type="button" onClick={() => onDelete(sound)} disabled={busy} title="Supprimer">Supprimer</button>
      </div>
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
  const [playing, setPlaying] = useState<{ id: string; totalFrames: number; progress: number } | null>(null);

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
        if (status.playbackTotalFrames !== playing.totalFrames) return;
        const progress = Math.min(1, status.playbackFrames / Math.max(1, playing.totalFrames));
        if (progress >= 1) setPlaying(null);
        else setPlaying((current) => current?.id === playing.id ? { ...current, progress } : current);
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
      setPlaying({ id: sound.id, totalFrames, progress: 0 });
    } catch (reason) {
      setError(String(reason));
    }
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
          <button className="primary-button" type="button" onClick={importSounds} disabled={!selected || busy}>{busy ? "Patientez…" : "Ajouter des sons"}</button>
        </header>
        {notice && <p className="notice-message" role="status">{notice}</p>}
        {error && <p className="error-message" role="alert">{error}</p>}
        {selected && selected.sounds.length > 0 ? (
          <div className="sound-grid">
            {selected.sounds.map((sound, index) => <SoundCard key={sound.id} sound={sound} index={index} total={selected.sounds.length} busy={busy} playing={playing?.id === sound.id} progress={playing?.id === sound.id ? playing.progress : 0} onPlay={playSound} onRename={renameSound} onImage={chooseImage} onDelete={deleteSound} onMove={moveSound} />)}
          </div>
        ) : (
          <div className="empty-state"><div className="empty-icon" aria-hidden="true">♪</div><h2>Votre soundboard est vide</h2><p>Ajoutez vos premiers sons pour les retrouver ici.</p><button className="primary-button" type="button" onClick={importSounds} disabled={!selected || busy}>Choisir des sons</button></div>
        )}
      </section>
    </div>
  );
}

function AudioPage() {
  const [microphones, setMicrophones] = useState<Microphone[]>([]); const [selectedId, setSelectedId] = useState("");
  const [status, setStatus] = useState<AudioStatus>(stoppedStatus); const [busy, setBusy] = useState(false);
  const [muted, setMuted] = useState(false); const [error, setError] = useState<string | null>(null);
  const isRunning = status.state === "running" || status.state === "recovering";
  const selectedMicrophone = microphones.find((microphone) => microphone.id === selectedId);
  useEffect(() => { void invoke<Microphone[]>("list_microphones").then((devices) => { setMicrophones(devices); setSelectedId((devices.find((device) => device.isDefault) ?? devices[0])?.id ?? ""); }).catch((reason: unknown) => setError(String(reason))); }, []);
  useEffect(() => { const refresh = () => void invoke<AudioStatus>("audio_status").then(setStatus).catch((reason: unknown) => setError(String(reason))); refresh(); const timer = window.setInterval(refresh, 500); return () => window.clearInterval(timer); }, []);
  async function startAudio() { if (!selectedId) return; setBusy(true); setError(null); try { await invoke("start_audio", { deviceId: selectedId }); setStatus(await invoke<AudioStatus>("audio_status")); } catch (reason) { setError(String(reason)); } finally { setBusy(false); } }
  async function stopAudio() { setBusy(true); try { await invoke("stop_audio"); setStatus(stoppedStatus); setMuted(false); } catch (reason) { setError(String(reason)); } finally { setBusy(false); } }
  async function toggleMute() { try { await invoke("set_microphone_muted", { muted: !muted }); setMuted(!muted); } catch (reason) { setError(String(reason)); } }
  return (
    <section className="workspace"><header className="topbar"><div><p className="eyebrow">Chemin audio</p><h1>Microphone virtuel</h1></div><span className={`status status-${status.state}`}><span className="status-dot" />{statusLabels[status.state]}</span></header>
      <section className="audio-panel" aria-labelledby="audio-title"><div className="panel-copy"><p className="eyebrow">Source physique</p><h2 id="audio-title">Choisissez votre microphone</h2><p>Votre voix et les sons seront réunis dans « SLB Virtual Microphone ».</p></div>
        <label className="field-label" htmlFor="microphone-select">Microphone d’entrée</label><select id="microphone-select" value={selectedId} onChange={(event) => setSelectedId(event.target.value)} disabled={isRunning || busy}>{microphones.length === 0 && <option value="">Aucun microphone détecté</option>}{microphones.map((microphone) => <option key={microphone.id} value={microphone.id}>{microphone.name}{microphone.isDefault ? " — par défaut" : ""}</option>)}</select>
        <div className="actions">{!isRunning ? <button className="primary-button" type="button" onClick={startAudio} disabled={!selectedId || busy}>{busy ? "Démarrage…" : "Démarrer"}</button> : <button className="danger-button" type="button" onClick={stopAudio} disabled={busy}>Arrêter</button>}<button className="secondary-button" type="button" onClick={toggleMute} disabled={!isRunning}>{muted ? "Réactiver ma voix" : "Couper ma voix"}</button><button className="secondary-button" type="button" onClick={() => invoke("play_reference_sound")} disabled={!isRunning}>Jouer le son test</button></div>
        <div className="signal-card"><div><span className="signal-label">Entrée</span><strong>{selectedMicrophone?.name ?? "Non sélectionnée"}</strong></div><div className="route-line"><span /></div><div><span className="signal-label">Sortie</span><strong>SLB Virtual Microphone</strong></div></div>
        {isRunning && <dl className="metrics"><div><dt>Format</dt><dd>{status.inputSampleRate ? `${status.inputSampleRate / 1000} kHz` : "—"}</dd></div><div><dt>Niveau</dt><dd>{Math.round(status.peak * 100)} %</dd></div><div><dt>File audio</dt><dd>{status.queuedFrames} trames</dd></div><div><dt>Reconnexions</dt><dd>{status.restartCount}</dd></div></dl>}{(error || status.lastError) && <p className="error-message" role="alert">{error ?? status.lastError}</p>}
      </section></section>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("library");
  return <main className="app-shell"><aside className="sidebar"><div className="brand-mark">SLB</div><nav aria-label="Navigation principale"><button className={`nav-item ${page === "library" ? "active" : ""}`} type="button" onClick={() => setPage("library")}>Mes soundboards</button><button className={`nav-item ${page === "audio" ? "active" : ""}`} type="button" onClick={() => setPage("audio")}>Audio</button><button className="nav-item" type="button" disabled>Communauté</button></nav></aside>{page === "library" ? <LibraryPage /> : <AudioPage />}</main>;
}
