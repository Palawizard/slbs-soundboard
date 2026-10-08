import { invoke } from "@tauri-apps/api/core";
import { CSSProperties, ReactNode, useEffect, useState } from "react";
import "./App.css";
import { AudioPage } from "./AudioPage";
import { AudioStatus, useAudioStatus } from "./audioStatus";
import { CommunityPage } from "./CommunityPage";
import { communityApi, useCommunityStore } from "./community";
import { communityEnabled } from "./features";
import { AudioIcon, ChevronIcon, CommunityIcon, SettingsIcon, SoundsIcon, StopIcon } from "./icons";
import { LibraryPage } from "./LibraryPage";
import { SettingsPage } from "./SettingsPage";
import { UpdateBanner } from "./updates";

type Page = "library" | "audio" | "community" | "settings";

function NavButton({ page, current, icon, label, onSelect }: { page: Page; current: Page; icon: ReactNode; label: string; onSelect: (page: Page) => void }) {
  return (
    <button className="rail-item" type="button" aria-current={current === page ? "page" : undefined} onClick={() => onSelect(page)}>
      {icon}<span>{label}</span>
    </button>
  );
}

/// The bottom strip answers the only question a player has mid-game:
/// is my voice reaching the call, and can I silence everything now.
function StatusStrip({ status, onOpenAudio }: { status: AudioStatus; onOpenAudio: () => void }) {
  const micLive = status.state === "running";
  const micState = micLive ? "ok" : status.state === "stopped" ? "off" : "warn";
  const cableState = status.virtualOutputConnected ? "ok" : status.virtualOutputDevice ? "warn" : "off";
  const level = { "--level": Math.min(1, Math.max(0, status.peak)) } as CSSProperties;
  const voices = status.activeVoices || status.activeSoundIds?.length || 0;
  const micText = micLive ? "Micro actif" : status.state === "stopped" ? "Micro arrêté" : status.state === "starting" ? "Micro en démarrage" : "Micro en reconnexion";
  const cableText = status.virtualOutputConnected ? (status.virtualOutputActiveDevice ?? "Câble connecté") : "Câble non connecté";
  return (
    <footer className="status-strip">
      <button className="strip-route" type="button" onClick={onOpenAudio} aria-label={`Chemin audio : ${micText}, ${cableText}. Ouvrir les réglages audio`}>
        <span className="strip-stop" data-state={micState}><span className="state-dot" />{micText}</span>
        <span className="strip-meter" style={level} aria-hidden="true"><span /></span>
        <ChevronIcon />
        <span className="strip-stop strip-mix">Mixage</span>
        <ChevronIcon />
        <span className="strip-stop" data-state={cableState}><span className="state-dot" /><span className="strip-cable">{cableText}</span></span>
      </button>
      <span className="strip-voices" role="status">{voices > 0 ? `${voices} son${voices > 1 ? "s" : ""} en cours` : ""}</span>
      <button className={`stop-all ${voices > 0 ? "armed" : ""}`} type="button" onClick={() => void invoke("stop_all_sounds").catch(() => undefined)}><StopIcon />Tout arrêter</button>
    </footer>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("library");
  const setSession = useCommunityStore((state) => state.setSession);
  const { status, setStatus } = useAudioStatus();
  useEffect(() => { if (communityEnabled) void communityApi.session().then(setSession).catch(() => setSession(null)); }, [setSession]);
  return (
    <main className="app-shell">
      <nav className="rail" aria-label="Navigation principale">
        <span className="brand" aria-hidden="true">SLB</span>
        <NavButton page="library" current={page} icon={<SoundsIcon />} label="Sons" onSelect={setPage} />
        <NavButton page="audio" current={page} icon={<AudioIcon />} label="Audio" onSelect={setPage} />
        {communityEnabled && <NavButton page="community" current={page} icon={<CommunityIcon />} label="Communauté" onSelect={setPage} />}
        <span className="rail-spacer" />
        <NavButton page="settings" current={page} icon={<SettingsIcon />} label="Réglages" onSelect={setPage} />
      </nav>
      <div className="workspace">
        {/* The library stays mounted on every page: it owns the global shortcuts. */}
        <div className="page-slot" hidden={page !== "library"}><LibraryPage audio={status} active={page === "library"} /></div>
        {page === "audio" ? <AudioPage status={status} setStatus={setStatus} />
          : page === "community" && communityEnabled ? <CommunityPage />
            : page === "settings" ? <SettingsPage /> : null}
      </div>
      <StatusStrip status={status} onOpenAudio={() => setPage("audio")} />
      <UpdateBanner />
    </main>
  );
}
