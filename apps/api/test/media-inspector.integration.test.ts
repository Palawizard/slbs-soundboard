import { spawnSync } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import sharp from "sharp";
import { afterEach, describe, expect, it } from "vitest";
import { ProductionMediaInspector } from "../src/media/inspector.js";

const roots: string[] = [];
const ffmpegAvailable = spawnSync("ffmpeg", ["-version"], { windowsHide: true }).status === 0
  && spawnSync("ffprobe", ["-version"], { windowsHide: true }).status === 0;

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe.skipIf(!ffmpegAvailable)("production media inspection", () => {
  it("fully decodes valid audio and images", async () => {
    const root = await mkdtemp(join(tmpdir(), "slb-inspector-")); roots.push(root);
    const audioPath = join(root, "sound.wav");
    const imagePath = join(root, "cover.png");
    await writeFile(audioPath, waveFile(8_000, 1));
    await sharp({ create: { width: 4, height: 3, channels: 3, background: "#2255aa" } }).png().toFile(imagePath);

    const inspector = new ProductionMediaInspector();
    await expect(inspector.inspectAudio(audioPath)).resolves.toMatchObject({ mimeType: "audio/wav", extension: "wav", durationMs: 1_000 });
    await expect(inspector.inspectImage(imagePath)).resolves.toMatchObject({ mimeType: "image/png", extension: "png", width: 4, height: 3 });
  });

  it("rejects a file whose claimed extension hides non-media content", async () => {
    const root = await mkdtemp(join(tmpdir(), "slb-inspector-")); roots.push(root);
    const hostilePath = join(root, "not-really-audio.wav");
    await writeFile(hostilePath, "<script>alert('not audio')</script>");
    await expect(new ProductionMediaInspector().inspectAudio(hostilePath)).rejects.toThrow("pris en charge");
  });
});

function waveFile(sampleRate: number, seconds: number): Buffer {
  const samples = sampleRate * seconds;
  const dataBytes = samples * 2;
  const buffer = Buffer.alloc(44 + dataBytes);
  buffer.write("RIFF", 0); buffer.writeUInt32LE(36 + dataBytes, 4); buffer.write("WAVE", 8);
  buffer.write("fmt ", 12); buffer.writeUInt32LE(16, 16); buffer.writeUInt16LE(1, 20);
  buffer.writeUInt16LE(1, 22); buffer.writeUInt32LE(sampleRate, 24); buffer.writeUInt32LE(sampleRate * 2, 28);
  buffer.writeUInt16LE(2, 32); buffer.writeUInt16LE(16, 34); buffer.write("data", 36); buffer.writeUInt32LE(dataBytes, 40);
  return buffer;
}
