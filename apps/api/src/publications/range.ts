export type ByteRange = { start: number; end: number; length: number };

export function parseByteRange(header: string | undefined, size: number): ByteRange | null {
  if (!header) return null;
  if (!Number.isSafeInteger(size) || size <= 0 || !header.startsWith("bytes=") || header.includes(",")) throw new RangeNotSatisfiableError(size);
  const match = /^bytes=(\d*)-(\d*)$/.exec(header);
  if (!match) throw new RangeNotSatisfiableError(size);
  const startText = match[1] ?? ""; const endText = match[2] ?? "";
  if (!startText && !endText) throw new RangeNotSatisfiableError(size);
  let start: number; let end: number;
  if (!startText) {
    const suffix = Number(endText);
    if (!Number.isSafeInteger(suffix) || suffix <= 0) throw new RangeNotSatisfiableError(size);
    start = Math.max(0, size - suffix); end = size - 1;
  } else {
    start = Number(startText); end = endText ? Number(endText) : size - 1;
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || start >= size || end < start) throw new RangeNotSatisfiableError(size);
    end = Math.min(end, size - 1);
  }
  return { start, end, length: end - start + 1 };
}

export class RangeNotSatisfiableError extends Error {
  constructor(public readonly size: number) { super("Requested byte range is not satisfiable"); }
}
