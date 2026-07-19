import { invoke } from "@tauri-apps/api/core";

export type Sound = {
  id: string;
  title: string;
  audioHash: string;
  audioExtension: string;
  durationMs: number;
  sampleRate: number;
  channels: number;
  imageHash: string | null;
  imageExtension: string | null;
  createdAtMs: number;
};

export type Soundboard = {
  id: string;
  title: string;
  position: number;
  sounds: Sound[];
};

export type LibrarySnapshot = {
  soundboards: Soundboard[];
  recoveryNotice: string | null;
};

export function moveItem<T>(items: T[], from: number, to: number): T[] {
  if (from < 0 || from >= items.length || to < 0 || to >= items.length || from === to) {
    return items;
  }
  const next = [...items];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export async function loadLibrary(): Promise<LibrarySnapshot> {
  return invoke<LibrarySnapshot>("library_snapshot");
}
