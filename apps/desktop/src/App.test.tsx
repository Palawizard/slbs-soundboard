import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-global-shortcut", () => ({ register: vi.fn(), unregister: vi.fn() }));

const sound = {
  id: "sound-1", title: "Cloche", audioHash: "abc", audioExtension: "wav",
  durationMs: 1_250, sampleRate: 48_000, channels: 2, imageHash: null,
  imageExtension: null, waveform: [0.2, 0.7, 0.4], createdAtMs: 1,
  playback: { volume: 1, pitchSemitones: 0, speed: 1, replayPolicy: "restart", keybind: null },
};

describe("library interface", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(register).mockReset();
    vi.mocked(unregister).mockReset();
    vi.mocked(register).mockResolvedValue();
    vi.mocked(unregister).mockResolvedValue();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "library_snapshot") return { soundboards: [{ id: "board-1", title: "Favoris", position: 0, sounds: [sound] }], recoveryNotice: null, activeSoundboardId: "board-1" };
      if (command === "play_sound") return 60_000;
      if (command === "audio_status") return { playbackFrames: 0, playbackTotalFrames: 60_000 };
      if (command === "create_soundboard") return { id: "board-2", title: "Jeux", position: 1, sounds: [] };
      return undefined;
    });
  });

  it("loads a soundboard and triggers an imported sound", async () => {
    render(<App />);
    expect(await screen.findByText("Cloche")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Jouer Cloche" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("play_sound", { soundId: "sound-1" }));
  });

  it("creates a soundboard from the library rail", async () => {
    render(<App />);
    const field = await screen.findByPlaceholderText("Nouveau soundboard");
    fireEvent.change(field, { target: { value: "Jeux" } });
    fireEvent.click(screen.getByRole("button", { name: "Créer le soundboard" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_soundboard", { title: "Jeux" }));
  });

  it("registers a persisted global shortcut and plays its sound on press", async () => {
    const boundSound = { ...sound, playback: { ...sound.playback, keybind: "Ctrl+Shift+K" } };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "library_snapshot") return { soundboards: [{ id: "board-1", title: "Favoris", position: 0, sounds: [boundSound] }], recoveryNotice: null, activeSoundboardId: "board-1" };
      if (command === "play_sound") return 60_000;
      return undefined;
    });
    const { unmount } = render(<App />);
    await waitFor(() => expect(register).toHaveBeenCalledWith(["Ctrl+Shift+K"], expect.any(Function)));
    const handler = vi.mocked(register).mock.calls[0][1];
    handler({ shortcut: "Ctrl+Shift+K", id: 1, state: "Pressed" });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("play_sound", { soundId: "sound-1" }));
    unmount();
    await waitFor(() => expect(unregister).toHaveBeenCalledWith(["Ctrl+Shift+K"]));
  });
});
