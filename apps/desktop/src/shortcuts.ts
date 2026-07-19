import { invoke } from "@tauri-apps/api/core";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { useEffect, useRef } from "react";
import { Soundboard } from "./library";

const modifierCodes = new Set(["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"]);
const namedKeys: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Escape: "Escape",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Delete",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
};

export function shortcutFromKeyboardEvent(event: Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string | null {
  if (modifierCodes.has(event.code)) return null;
  let key: string | undefined;
  if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3);
  else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
  else if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(event.code)) key = event.code;
  else key = namedKeys[event.code];
  if (!key) return null;

  const modifiers = [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "Super"].filter(Boolean);
  return modifiers.length > 0 ? [...modifiers, key].join("+") : null;
}

export function shortcutLabel(shortcut: string | null): string {
  if (!shortcut) return "Aucun raccourci";
  return shortcut.replace("Shift", "Maj").replace("Super", "Windows");
}

export function useGlobalShortcuts(soundboards: Soundboard[], onError: (message: string) => void) {
  const generation = useRef(0);
  useEffect(() => {
    const entries = soundboards.flatMap((board) => board.sounds)
      .filter((sound) => sound.playback.keybind)
      .map((sound) => [sound.playback.keybind as string, sound.id] as const);
    const shortcuts = entries.map(([shortcut]) => shortcut);
    const soundIds = new Map(entries.map(([shortcut, id]) => [shortcut.toLowerCase(), id]));
    const currentGeneration = ++generation.current;
    let registered = false;

    const synchronize = async () => {
      if (shortcuts.length === 0) return;
      try {
        await register(shortcuts, (event) => {
          if (event.state !== "Pressed") return;
          const soundId = soundIds.get(event.shortcut.toLowerCase());
          if (soundId) void invoke("play_sound", { soundId }).catch((reason: unknown) => onError(String(reason)));
        });
        registered = true;
        if (currentGeneration !== generation.current) {
          await unregister(shortcuts);
          registered = false;
        }
      } catch (reason) {
        onError(`Impossible d’enregistrer un raccourci global : ${String(reason)}`);
      }
    };
    void synchronize();

    return () => {
      generation.current += 1;
      if (registered) void unregister(shortcuts).catch((reason: unknown) => onError(`Impossible de libérer un raccourci global : ${String(reason)}`));
    };
  }, [soundboards, onError]);
}
