import { Channel, invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

export type PublicUser = { id: string; username: string; avatarUrl: string | null; createdAt: string };
export type SessionView = { expiresAt: string | null; user: PublicUser };
export type OwnedMedia = { id: string; kind: "audio" | "image"; hash: string; mimeType: string; byteSize: number; durationMs: number | null; width: number | null; height: number | null; createdAt: string };
export type CommunitySound = { id: string; title: string; description: string; owner: PublicUser; audio: OwnedMedia; image: OwnedMedia | null; createdAt: string; updatedAt: string };
export type PublicationList = { items: CommunitySound[]; nextCursor: string | null };
export type TransferProgress = { stage: "audio" | "image" | "publication"; transferredBytes: number; totalBytes: number; percent: number };
export type DriverStatus = { installed: boolean; packageAvailable: boolean; requiresRestart: boolean };

type CommunityState = {
  session: SessionView | null;
  previewId: string | null;
  progress: Record<string, TransferProgress>;
  setSession: (session: SessionView | null) => void;
  setPreview: (id: string | null) => void;
  setProgress: (id: string, progress: TransferProgress | null) => void;
};

export const useCommunityStore = create<CommunityState>((set) => ({
  session: null,
  previewId: null,
  progress: {},
  setSession: (session) => set({ session }),
  setPreview: (previewId) => set({ previewId }),
  setProgress: (id, progress) => set((state) => {
    const next = { ...state.progress };
    if (progress) next[id] = progress; else delete next[id];
    return { progress: next };
  }),
}));

export const communityApi = {
  apiUrl: () => invoke<string>("community_api_url"),
  session: () => invoke<SessionView | null>("community_session"),
  login: () => invoke<SessionView>("community_login"),
  logout: () => invoke<void>("community_logout"),
  updateUsername: (username: string) => invoke<PublicUser>("community_update_username", { username }),
  browse: async (query: string, cursor: string | null, signal?: AbortSignal) => {
    const result = await invoke<PublicationList>("community_browse", { query: query || null, cursor });
    if (signal?.aborted) throw new DOMException("Request cancelled", "AbortError");
    return result;
  },
  owned: () => invoke<CommunitySound[]>("community_owned_publications"),
  publish: (soundId: string, description: string, onProgress: (value: TransferProgress) => void) => {
    const progress = new Channel<TransferProgress>();
    progress.onmessage = onProgress;
    return invoke<CommunitySound>("community_publish_sound", { soundId, description, progress });
  },
  remove: (publicationId: string) => invoke<void>("community_delete_publication", { publicationId }),
  import: (publicationId: string, soundboardId: string, onProgress: (value: TransferProgress) => void) => {
    const progress = new Channel<TransferProgress>();
    progress.onmessage = onProgress;
    return invoke("community_import_sound", { publicationId, soundboardId, progress });
  },
  diagnosticsEnabled: () => invoke<boolean>("diagnostics_settings"),
  setDiagnosticsEnabled: (enabled: boolean) => invoke<void>("set_diagnostics_settings", { enabled }),
  readDiagnostics: () => invoke<string>("read_diagnostics"),
  clearDiagnostics: () => invoke<void>("clear_diagnostics"),
  driverStatus: () => invoke<DriverStatus>("driver_status"),
  installDriver: () => invoke<void>("install_driver"),
  removeDriver: () => invoke<void>("remove_driver"),
};

export function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} Ko`;
  return `${(bytes / 1024 / 1024).toFixed(1).replace(".", ",")} Mo`;
}
