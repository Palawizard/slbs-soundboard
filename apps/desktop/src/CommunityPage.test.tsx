import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { useCommunityStore } from "./community";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class { onmessage?: (value: unknown) => void },
}));
// The community surface ships disabled; these tests cover it as it behaves once
// the server is deployed and VITE_SLB_COMMUNITY is set at build time.
vi.mock("./features", () => ({ communityEnabled: true, driverEnabled: true }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-global-shortcut", () => ({ register: vi.fn(async () => undefined), unregister: vi.fn(async () => undefined) }));

const user = { id: "123e4567-e89b-12d3-a456-426614174000", username: "SLB_User", avatarUrl: null, createdAt: "2026-07-19T10:00:00.000Z" };
const publication = {
  id: "423e4567-e89b-12d3-a456-426614174001", title: "Cloche distante", description: "Une cloche claire", owner: user,
  audio: { id: "223e4567-e89b-12d3-a456-426614174000", kind: "audio", hash: "a".repeat(64), mimeType: "audio/wav", byteSize: 12_000, durationMs: 1_200, width: null, height: null, createdAt: "2026-07-19T10:00:00.000Z" },
  image: null, createdAt: "2026-07-19T10:00:00.000Z", updatedAt: "2026-07-19T10:00:00.000Z",
};
const localSound = { id: "sound-1", title: "Son local", audioHash: "b".repeat(64), audioExtension: "wav", durationMs: 500, sampleRate: 48_000, channels: 2, imageHash: null, imageExtension: null, waveform: [0.2], playback: { volume: 1, pitchSemitones: 0, speed: 1, replayPolicy: "restart", keybind: null }, createdAtMs: 1, publicationId: null };

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}><App /></QueryClientProvider>);
}

describe("community workflows", () => {
  beforeEach(() => {
    useCommunityStore.setState({ session: null, previewId: null, progress: {} });
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "community_session") return { expiresAt: null, user };
      if (command === "community_api_url") return "https://community.example.test";
      if (command === "community_browse") return { items: [publication], nextCursor: null };
      if (command === "community_owned_publications") return [];
      if (command === "library_snapshot") return { soundboards: [{ id: "board-1", title: "Favoris", position: 0, sounds: [localSound] }], recoveryNotice: null, activeSoundboardId: "board-1" };
      if (command === "community_import_sound") return localSound;
      if (command === "community_publish_sound") return publication;
      if (command === "community_update_username") return { ...user, username: "Nouveau_Nom" };
      if (command === "diagnostics_settings") return true;
      if (command === "driver_status") return { installed: false, packageAvailable: false, requiresRestart: false };
      return undefined;
    });
  });

  it("browses, imports and explicitly publishes sounds", async () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Communauté" }));
    expect(await screen.findByText("Cloche distante")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Importer" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("community_import_sound", expect.objectContaining({ publicationId: publication.id, soundboardId: "board-1" })));
    fireEvent.click(screen.getByRole("button", { name: "Publier" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("community_publish_sound", expect.objectContaining({ soundId: "sound-1" })));
  });

  it("has no automatically detectable accessibility violation", async () => {
    const { container } = renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Communauté" }));
    await screen.findByText("Cloche distante");
    const results = await axe.run(container, { rules: { "color-contrast": { enabled: false } } });
    expect(results.violations).toEqual([]);
  });

  it("edits the account and exposes local-only privacy controls", async () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Réglages" }));
    expect(await screen.findByText(/Gestionnaire d’identifiants Windows/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Pseudonyme unique"), { target: { value: "Nouveau_Nom" } });
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("community_update_username", { username: "Nouveau_Nom" }));
    expect(screen.getByText(/Aucune télémétrie n’est envoyée/)).toBeInTheDocument();
  });
});
