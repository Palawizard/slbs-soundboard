import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileTypeFromFile } from "file-type";
import sharp from "sharp";
import type { MediaMetadata } from "../domain.js";
import { DomainError } from "../domain.js";
import { MAX_SOUND_DURATION_MS } from "@slb/contracts";

const execFileAsync = promisify(execFile);
const audioTypes = new Map([
  ["mp3", "audio/mpeg"], ["wav", "audio/wav"], ["flac", "audio/flac"],
  ["ogg", "audio/ogg"], ["m4a", "audio/mp4"], ["aac", "audio/aac"],
]);
const imageTypes = new Map([
  ["png", "image/png"], ["jpg", "image/jpeg"], ["webp", "image/webp"],
  ["gif", "image/gif"], ["bmp", "image/bmp"],
]);

export type InspectedMedia = Omit<MediaMetadata, "kind" | "hash" | "byteSize" | "storageKey">;

export interface MediaInspector {
  inspectAudio(path: string): Promise<InspectedMedia>;
  inspectImage(path: string): Promise<InspectedMedia>;
}

export class ProductionMediaInspector implements MediaInspector {
  constructor(
    private readonly ffprobePath = "ffprobe",
    private readonly ffmpegPath = "ffmpeg",
  ) {}

  async inspectAudio(path: string): Promise<InspectedMedia> {
    const signature = await fileTypeFromFile(path);
    const mimeType = signature ? audioTypes.get(signature.ext) : undefined;
    if (!signature || !mimeType) throw new DomainError("invalid", "Ce fichier audio n’est pas pris en charge.");
    let stdout: string;
    try {
      ({ stdout } = await execFileAsync(this.ffprobePath, [
        "-v", "error", "-show_entries", "format=duration", "-show_entries", "stream=codec_type",
        "-of", "json", path,
      ], { timeout: 15_000, maxBuffer: 512 * 1024, windowsHide: true }));
    } catch { throw new DomainError("invalid", "Ce fichier audio ne peut pas être décodé."); }
    const probe = JSON.parse(stdout) as { format?: { duration?: string }; streams?: { codec_type?: string }[] };
    if (!probe.streams?.some((stream) => stream.codec_type === "audio") || probe.streams.some((stream) => stream.codec_type === "video")) {
      throw new DomainError("invalid", "Ce fichier ne contient pas uniquement un son valide.");
    }
    const durationMs = Math.round(Number(probe.format?.duration) * 1000);
    if (!Number.isFinite(durationMs) || durationMs <= 0 || durationMs > MAX_SOUND_DURATION_MS) {
      throw new DomainError("invalid", durationMs > MAX_SOUND_DURATION_MS ? "Ce son dépasse dix minutes." : "La durée de ce son est invalide.");
    }
    try {
      await execFileAsync(this.ffmpegPath, ["-v", "error", "-nostdin", "-i", path, "-map", "0:a:0", "-f", "null", "-"], {
        timeout: 60_000, maxBuffer: 1024 * 1024, windowsHide: true,
      });
    } catch { throw new DomainError("invalid", "Le contenu audio est endommagé."); }
    return { mimeType, extension: signature.ext, durationMs, width: null, height: null };
  }

  async inspectImage(path: string): Promise<InspectedMedia> {
    const signature = await fileTypeFromFile(path);
    const mimeType = signature ? imageTypes.get(signature.ext) : undefined;
    if (!signature || !mimeType) throw new DomainError("invalid", "Cette image n’est pas prise en charge.");
    try {
      const image = sharp(path, { limitInputPixels: 40_000_000, animated: true, pages: 2 });
      const metadata = await image.metadata();
      if (!metadata.width || !metadata.height || (metadata.pages ?? 1) > 1 || metadata.width > 8192 || metadata.height > 8192) {
        throw new DomainError("invalid", "Les dimensions de cette image ne sont pas prises en charge.");
      }
      await sharp(path, { limitInputPixels: 40_000_000 }).resize(1, 1).raw().toBuffer();
      return { mimeType, extension: signature.ext, durationMs: null, width: metadata.width, height: metadata.height };
    } catch (error) {
      if (error instanceof DomainError) throw error;
      throw new DomainError("invalid", "Cette image ne peut pas être décodée.");
    }
  }
}
