import { invoke } from "@tauri-apps/api/core";
import { ReactNode, useCallback, useEffect, useState } from "react";
import { AudioStatus, statusLabels } from "./audioStatus";
import { AudioIcon, CableIcon, HeadphonesIcon, MixIcon, RefreshIcon } from "./icons";

type Microphone = { id: string; name: string; isDefault: boolean };
type OutputDevice = { name: string; isVirtualCable: boolean };
type MixControlBus = "microphone" | "soundboard" | "master";
type MixSettings = {
  microphoneGain: number; microphoneMuted: boolean;
  soundboardGain: number; soundboardMuted: boolean;
  masterGain: number; masterMuted: boolean;
  monitorEnabled: boolean; monitorGain: number; monitorMuted: boolean;
};
const defaultMix: MixSettings = {
  microphoneGain: 1, microphoneMuted: false, soundboardGain: 1, soundboardMuted: false,
  masterGain: 1, masterMuted: false, monitorEnabled: true, monitorGain: 1, monitorMuted: false,
};
const busLabels: Record<MixControlBus, string> = {
  microphone: "Votre voix", soundboard: "Vos sons", master: "Volume envoyé",
};
const busKeys: Record<MixControlBus, { gain: keyof MixSettings; muted: keyof MixSettings }> = {
  microphone: { gain: "microphoneGain", muted: "microphoneMuted" },
  soundboard: { gain: "soundboardGain", muted: "soundboardMuted" },
  master: { gain: "masterGain", muted: "masterMuted" },
};

type Stop = { key: string; icon: ReactNode; label: string; value: string; state: "ok" | "warn" | "off" | "info" };

function Fader({ id, label, value, muted, disabled, onChange, onMute }: { id: string; label: string; value: number; muted: boolean; disabled?: boolean; onChange: (value: number) => void; onMute: () => void }) {
  return (
    <div className="fader" data-muted={muted || undefined}>
      <label htmlFor={id}>{label}</label>
      <input id={id} type="range" min="0" max="2" step="0.05" value={value} onChange={(event) => onChange(Number(event.target.value))} />
      <output htmlFor={id}>{muted ? "coupé" : `${Math.round(value * 100)} %`}</output>
      <button type="button" className="toggle-button" aria-pressed={muted} aria-label={`Couper ${label.toLocaleLowerCase("fr")}`} onClick={onMute} disabled={disabled}>Couper</button>
    </div>
  );
}

export function AudioPage({ status, setStatus }: { status: AudioStatus; setStatus: (status: AudioStatus) => void }) {
  const [microphones, setMicrophones] = useState<Microphone[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [busy, setBusy] = useState(false);
  const [mix, setMix] = useState<MixSettings>(defaultMix);
  const [outputs, setOutputs] = useState<OutputDevice[]>([]);
  const [virtualOutput, setVirtualOutput] = useState("");
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
      setVirtualOutput(stored ?? "");
    } catch (reason) { setError(String(reason)); }
  }, []);
  useEffect(() => { void refreshOutputs(); }, [refreshOutputs]);
  // The levels live in the library database, not in this component.
  useEffect(() => { void invoke<MixSettings>("mix_settings").then((value) => setMix({ ...defaultMix, ...value })).catch((reason: unknown) => setError(String(reason))); }, []);
  const changeMix = useCallback(async (patch: Partial<MixSettings>) => {
    const next = { ...mix, ...patch };
    setMix(next);
    try { await invoke("set_mix_settings", { settings: next }); }
    catch (reason) { setError(String(reason)); }
  }, [mix]);
  async function changeVirtualOutput(name: string) {
    setVirtualOutput(name);
    try { await invoke("set_virtual_output_device", { device: name || null }); }
    catch (reason) { setError(String(reason)); }
  }
  // The engine is started by the application itself; this only switches device.
  async function changeMicrophone(deviceId: string) {
    setSelectedId(deviceId); setBusy(true); setError(null);
    try {
      await invoke("start_audio", { deviceId: deviceId || null });
      const value = await invoke<Partial<AudioStatus> | undefined>("audio_status");
      setStatus({ ...status, ...value });
    } catch (reason) { setError(String(reason)); } finally { setBusy(false); }
  }

  const cableTarget = virtualOutput ? virtualOutput.replace("Input", "Output") : null;
  const stops: Stop[] = [
    { key: "mic", icon: <AudioIcon />, label: "Micro", value: selectedMicrophone?.name ?? "Aucun micro", state: status.state === "running" ? "ok" : isRunning || status.state === "starting" ? "warn" : "off" },
    { key: "mix", icon: <MixIcon />, label: "Mixage", value: mix.masterMuted ? "Envoi coupé" : `Envoi ${Math.round(mix.masterGain * 100)} %`, state: mix.masterMuted ? "warn" : "ok" },
    { key: "cable", icon: <CableIcon />, label: "Câble virtuel", value: status.virtualOutputConnected ? (status.virtualOutputActiveDevice ?? virtualOutput) : virtualOutput ? "Non connecté" : "Aucun câble choisi", state: status.virtualOutputConnected ? "ok" : virtualOutput ? "warn" : "off" },
    // The app cannot see which microphone Discord uses: this stop is an instruction, never a status.
    { key: "app", icon: <HeadphonesIcon />, label: "Dans Discord ou le jeu", value: cableTarget ? `Micro : ${cableTarget}` : "Choisissez d’abord un câble", state: "info" },
  ];
  const problem = error ?? status.lastError ?? status.virtualOutputLastError ?? (mix.monitorEnabled ? status.monitorLastError : null);

  return (
    <section className="page audio-page" aria-labelledby="audio-page-title">
      <header className="page-head">
        <h1 id="audio-page-title">Audio</h1>
        <p>Votre voix et vos sons sont mélangés, puis envoyés dans le câble virtuel que Discord ou votre jeu utilise comme micro.</p>
      </header>

      <ol className="route" aria-label="Chemin du son">
        {stops.map((stop) => (
          <li key={stop.key} className="route-stop" data-state={stop.state}>
            <span className="route-node">{stop.icon}</span>
            <span className="route-label">{stop.label}</span>
            <span className="route-value" title={stop.value}>{stop.value}</span>
          </li>
        ))}
      </ol>

      {problem && <p className="message message-error" role="alert">{problem}</p>}

      <div className="audio-columns">
        <section className="panel" aria-labelledby="input-title">
          <div className="panel-head">
            <h2 id="input-title">Entrée</h2>
            <span className="state-chip" data-state={status.state}><span className="state-dot" />{statusLabels[status.state]}</span>
          </div>
          <div className="field">
            <label htmlFor="microphone-select">Microphone d’entrée</label>
            <select id="microphone-select" value={selectedId} onChange={(event) => void changeMicrophone(event.target.value)} disabled={busy}>
              {microphones.length === 0 && <option value="">Aucun microphone détecté</option>}
              {microphones.map((microphone) => <option key={microphone.id} value={microphone.id}>{microphone.name}{microphone.isDefault ? " — par défaut" : ""}</option>)}
            </select>
            <p className="field-hint">Le micro est actif dès l’ouverture de l’application, il n’y a rien à démarrer.</p>
          </div>
          <div className="button-row">
            {!isRunning && <button className="primary-button" type="button" onClick={() => void changeMicrophone(selectedId)} disabled={busy}>{busy ? "Connexion…" : "Réessayer"}</button>}
            <button className="ghost-button" type="button" onClick={() => void invoke("play_reference_sound").catch((reason: unknown) => setError(String(reason)))} disabled={!isRunning}>Jouer le son test</button>
          </div>
        </section>

        <section className="panel" aria-labelledby="routing-title">
          <div className="panel-head">
            <h2 id="routing-title">Sortie vers le micro virtuel</h2>
            <span className="state-chip" data-state={status.virtualOutputConnected ? "running" : "stopped"}><span className="state-dot" />{status.virtualOutputConnected ? "Connecté" : "Non connecté"}</span>
          </div>
          <div className="field">
            <label htmlFor="virtual-output-select">Câble virtuel</label>
            <div className="select-row">
              <select id="virtual-output-select" value={virtualOutput} onChange={(event) => void changeVirtualOutput(event.target.value)} disabled={busy}>
                <option value="">Aucune sortie</option>
                {outputs.map((device) => <option key={device.name} value={device.name}>{device.name}{device.isVirtualCable ? " — câble virtuel" : ""}</option>)}
              </select>
              <button type="button" className="icon-button" onClick={() => void refreshOutputs()} aria-label="Actualiser la liste" title="Actualiser la liste"><RefreshIcon /></button>
            </div>
          </div>
          {!hasCable
            ? <p className="callout">Aucun câble virtuel détecté. Installez VB-CABLE, un logiciel gratuit (donationware) publié par VB-Audio, puis actualisez la liste. <button type="button" className="link-button" onClick={() => void invoke("open_virtual_cable_download").catch((reason: unknown) => setError(String(reason)))}>Télécharger VB-CABLE</button></p>
            : cableTarget
              ? <p className="callout">Dans Discord ou votre jeu, choisissez <strong>{cableTarget}</strong> comme microphone.</p>
              : <p className="callout">Choisissez le câble ci-dessus pour que vos sons sortent dans vos appels.</p>}
        </section>
      </div>

      <section className="panel" aria-labelledby="mix-title">
        <div className="panel-head">
          <h2 id="mix-title">Ce que vos interlocuteurs entendent</h2>
        </div>
        <div className="faders">
          {(["microphone", "soundboard", "master"] as const).map((bus) => {
            const keys = busKeys[bus];
            return <Fader key={bus} id={`gain-${bus}`} label={busLabels[bus]} value={mix[keys.gain] as number} muted={mix[keys.muted] as boolean} onChange={(value) => void changeMix({ [keys.gain]: value })} onMute={() => void changeMix({ [keys.muted]: !(mix[keys.muted] as boolean) })} />;
          })}
        </div>
        <div className="monitor">
          <div className="monitor-copy">
            <h3 id="monitor-title">Écoute dans vos écouteurs</h3>
            <p>{status.monitorDeviceName ?? "Sortie par défaut"}{status.monitorRestartCount > 0 ? ` · ${status.monitorRestartCount} reconnexion${status.monitorRestartCount > 1 ? "s" : ""}` : ""}. Ne change que ce que vous entendez, jamais ce qui part dans vos appels.</p>
          </div>
          <button className="switch" type="button" role="switch" aria-checked={mix.monitorEnabled} aria-labelledby="monitor-title" onClick={() => void changeMix({ monitorEnabled: !mix.monitorEnabled })}><span className="switch-thumb" /></button>
        </div>
        <div className="faders">
          <Fader id="monitor-gain" label="Volume d’écoute" value={mix.monitorGain} muted={mix.monitorMuted} disabled={!mix.monitorEnabled} onChange={(value) => void changeMix({ monitorGain: value })} onMute={() => void changeMix({ monitorMuted: !mix.monitorMuted })} />
        </div>
        <p className="field-hint">Ces niveaux sont conservés d’une session à l’autre.</p>
      </section>

      {isRunning && (
        <dl className="readouts" aria-label="Diagnostic du flux">
          <div><dt>Format</dt><dd>{status.inputSampleRate ? `${status.inputSampleRate / 1000} kHz` : "—"}</dd></div>
          <div><dt>Écrêtages</dt><dd>{status.clippedSamples}</dd></div>
          <div><dt>File virtuelle</dt><dd>{status.queuedFrames} trames</dd></div>
          <div><dt>Reconnexions</dt><dd>{status.restartCount}</dd></div>
        </dl>
      )}
    </section>
  );
}
