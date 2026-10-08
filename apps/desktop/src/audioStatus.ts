import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

export type AudioStatus = {
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
  activeSoundIds: string[];
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

export const stoppedStatus: AudioStatus = {
  state: "stopped", deviceId: null, inputSampleRate: null, inputChannels: null,
  restartCount: 0, lastError: null, peak: 0, clippedSamples: 0, queuedFrames: 0,
  overrunFrames: 0, underrunFrames: 0,
  playbackFrames: 0, playbackTotalFrames: 0,
  playbackPaused: false, activeVoices: 0, activeSoundIds: [],
  monitorEnabled: false, monitorMuted: false, monitorGain: 1, monitorDeviceName: null,
  monitorRestartCount: 0, monitorLastError: null, monitorQueuedFrames: 0,
  monitorDroppedFrames: 0, monitorUnderrunFrames: 0,
  virtualOutputDevice: null, virtualOutputActiveDevice: null, virtualOutputConnected: false,
  virtualOutputLastError: null, virtualOutputDroppedFrames: 0,
};

export const statusLabels: Record<AudioStatus["state"], string> = {
  starting: "Démarrage", running: "Actif", recovering: "Reconnexion", stopped: "Arrêté",
};

/// One poll of the engine for the whole window. It runs fast only while
/// something plays, slows down at rest and stops while the window is hidden
/// in the tray, so the interface costs nothing during a game.
export function useAudioStatus() {
  const [status, setStatus] = useState<AudioStatus>(stoppedStatus);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let timer = 0;
    let cancelled = false;
    let inFlight = false;
    async function poll() {
      if (inFlight || cancelled) return;
      inFlight = true;
      window.clearTimeout(timer);
      let delay = 1000;
      if (document.visibilityState !== "hidden") {
        delay = 500;
        try {
          const value = await invoke<Partial<AudioStatus> | undefined>("audio_status");
          const next = { ...stoppedStatus, ...value };
          if (!cancelled) { setStatus(next); setError(null); }
          if (next.activeVoices > 0 || (next.activeSoundIds?.length ?? 0) > 0) delay = 200;
        } catch (reason) {
          if (!cancelled) setError(String(reason));
        }
      }
      inFlight = false;
      if (!cancelled) timer = window.setTimeout(poll, delay);
    }
    const wake = () => { if (document.visibilityState === "visible") void poll(); };
    void poll();
    document.addEventListener("visibilitychange", wake);
    return () => { cancelled = true; window.clearTimeout(timer); document.removeEventListener("visibilitychange", wake); };
  }, []);
  return { status, error, setStatus };
}
