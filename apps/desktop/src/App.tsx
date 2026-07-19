import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import "./App.css";

type Microphone = {
  id: string;
  name: string;
  isDefault: boolean;
};

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
};

const stoppedStatus: AudioStatus = {
  state: "stopped",
  deviceId: null,
  inputSampleRate: null,
  inputChannels: null,
  restartCount: 0,
  lastError: null,
  peak: 0,
  clippedSamples: 0,
  queuedFrames: 0,
  overrunFrames: 0,
  underrunFrames: 0,
};

const statusLabels: Record<AudioStatus["state"], string> = {
  starting: "Démarrage",
  running: "Actif",
  recovering: "Reconnexion",
  stopped: "Arrêté",
};

function App() {
  const [microphones, setMicrophones] = useState<Microphone[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [status, setStatus] = useState<AudioStatus>(stoppedStatus);
  const [busy, setBusy] = useState(false);
  const [muted, setMuted] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isRunning = status.state === "running" || status.state === "recovering";
  const selectedMicrophone = useMemo(
    () => microphones.find((microphone) => microphone.id === selectedId),
    [microphones, selectedId],
  );

  useEffect(() => {
    void invoke<Microphone[]>("list_microphones")
      .then((devices) => {
        setMicrophones(devices);
        const preferred = devices.find((device) => device.isDefault) ?? devices[0];
        setSelectedId(preferred?.id ?? "");
      })
      .catch((reason: unknown) => setError(String(reason)));
  }, []);

  useEffect(() => {
    const refresh = () => {
      void invoke<AudioStatus>("audio_status")
        .then(setStatus)
        .catch((reason: unknown) => setError(String(reason)));
    };
    refresh();
    const timer = window.setInterval(refresh, 500);
    return () => window.clearInterval(timer);
  }, []);

  async function startAudio() {
    if (!selectedId) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("start_audio", { deviceId: selectedId });
      setStatus(await invoke<AudioStatus>("audio_status"));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function stopAudio() {
    setBusy(true);
    try {
      await invoke("stop_audio");
      setStatus(stoppedStatus);
      setMuted(false);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function toggleMute() {
    const nextMuted = !muted;
    try {
      await invoke("set_microphone_muted", { muted: nextMuted });
      setMuted(nextMuted);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function playReferenceSound() {
    try {
      await invoke("play_reference_sound");
    } catch (reason) {
      setError(String(reason));
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-mark">SLB</div>
        <nav aria-label="Navigation principale">
          <button className="nav-item active" type="button">Audio</button>
          <button className="nav-item" type="button" disabled>Mes soundboards</button>
          <button className="nav-item" type="button" disabled>Communauté</button>
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">Chemin audio</p>
            <h1>Microphone virtuel</h1>
          </div>
          <span className={`status status-${status.state}`}>
            <span className="status-dot" aria-hidden="true" />
            {statusLabels[status.state]}
          </span>
        </header>

        <section className="audio-panel" aria-labelledby="audio-title">
          <div className="panel-copy">
            <p className="eyebrow">Source physique</p>
            <h2 id="audio-title">Choisissez votre microphone</h2>
            <p>
              Votre voix et les sons seront réunis dans « SLB Virtual Microphone ».
            </p>
          </div>

          <label className="field-label" htmlFor="microphone-select">Microphone d’entrée</label>
          <select
            id="microphone-select"
            value={selectedId}
            onChange={(event) => setSelectedId(event.target.value)}
            disabled={isRunning || busy}
          >
            {microphones.length === 0 && <option value="">Aucun microphone détecté</option>}
            {microphones.map((microphone) => (
              <option key={microphone.id} value={microphone.id}>
                {microphone.name}{microphone.isDefault ? " — par défaut" : ""}
              </option>
            ))}
          </select>

          <div className="actions">
            {!isRunning ? (
              <button className="primary-button" type="button" onClick={startAudio} disabled={!selectedId || busy}>
                {busy ? "Démarrage…" : "Démarrer"}
              </button>
            ) : (
              <button className="danger-button" type="button" onClick={stopAudio} disabled={busy}>
                Arrêter
              </button>
            )}
            <button className="secondary-button" type="button" onClick={toggleMute} disabled={!isRunning}>
              {muted ? "Réactiver ma voix" : "Couper ma voix"}
            </button>
            <button className="secondary-button" type="button" onClick={playReferenceSound} disabled={!isRunning}>
              Jouer le son test
            </button>
          </div>

          <div className="signal-card">
            <div>
              <span className="signal-label">Entrée</span>
              <strong>{selectedMicrophone?.name ?? "Non sélectionnée"}</strong>
            </div>
            <div className="route-line" aria-hidden="true"><span /></div>
            <div>
              <span className="signal-label">Sortie</span>
              <strong>SLB Virtual Microphone</strong>
            </div>
          </div>

          {isRunning && (
            <dl className="metrics">
              <div><dt>Format</dt><dd>{status.inputSampleRate ? `${status.inputSampleRate / 1000} kHz` : "—"}</dd></div>
              <div><dt>Niveau</dt><dd>{Math.round(status.peak * 100)} %</dd></div>
              <div><dt>File audio</dt><dd>{status.queuedFrames} trames</dd></div>
              <div><dt>Reconnexions</dt><dd>{status.restartCount}</dd></div>
            </dl>
          )}

          {(error || status.lastError) && (
            <p className="error-message" role="alert">{error ?? status.lastError}</p>
          )}
        </section>
      </section>
    </main>
  );
}

export default App;
