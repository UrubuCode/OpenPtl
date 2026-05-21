import lz4 from "lz4js";

const VIDEO_PACKET_MAGIC = [0x54, 0x56, 0x4e, 0x56] as const; // TVNV
const CURSOR_PACKET_MAGIC = [0x54, 0x56, 0x4e, 0x43] as const; // TVNC
const PACKET_VERSION = 1;
const VIDEO_PACKET_HEADER_SIZE = 32;
const VIDEO_RECT_HEADER_SIZE = 20;
const CURSOR_PACKET_HEADER_SIZE = 28;
const COMPRESSION_NONE = 0;
const COMPRESSION_LZ4 = 1;
const VNC_VIDEO_RECTS_EVENT = "openptl:vnc-video-rects";
const VNC_CURSOR_EVENT = "openptl:vnc-cursor";

function readU64(view: DataView, offset: number): number {
  const low = view.getUint32(offset, true);
  const high = view.getUint32(offset + 4, true);
  return high * 2 ** 32 + low;
}

function hasMagic(bytes: Uint8Array, magic: readonly number[]): boolean {
  if (bytes.byteLength < magic.length) {
    return false;
  }
  return magic.every((value, index) => bytes[index] === value);
}

function ensureUint8Array(data: Uint8Array | number[]): Uint8Array {
  return data instanceof Uint8Array ? data : new Uint8Array(data);
}

function decompressPayload(compression: number, payload: Uint8Array, expectedSize: number): Uint8Array | null {
  if (compression === COMPRESSION_NONE) {
    return payload;
  }
  if (compression !== COMPRESSION_LZ4) {
    return null;
  }

  try {
    const decoded = ensureUint8Array(lz4.decompress(payload));
    if (expectedSize > 0 && decoded.byteLength !== expectedSize) {
      return null;
    }
    return decoded;
  } catch {
    return null;
  }
}

export interface ParsedVncVideoRect {
  x: number;
  y: number;
  width: number;
  height: number;
  pixels: Uint8Array;
}

export interface ParsedVncVideoRectsPacket {
  frameId: number;
  width: number;
  height: number;
  ptsUs: number;
  frameBegin: boolean;
  frameEnd: boolean;
  rects: ParsedVncVideoRect[];
}

export interface ParsedVncCursorPacket {
  kind: "default" | "hidden" | "position" | "bitmap";
  x: number;
  y: number;
  hotspotX: number;
  hotspotY: number;
  width: number;
  height: number;
  bitmap?: Uint8Array;
}

export interface VncVideoRectsEventDetail {
  blockId: string;
  packet: ParsedVncVideoRectsPacket;
}

export interface VncCursorEventDetail {
  blockId: string;
  packet: ParsedVncCursorPacket;
}

export function parseVncVideoRectsPacket(message: ArrayBuffer): ParsedVncVideoRectsPacket | null {
  const bytes = new Uint8Array(message);
  if (bytes.byteLength < VIDEO_PACKET_HEADER_SIZE || !hasMagic(bytes, VIDEO_PACKET_MAGIC)) {
    return null;
  }

  const view = new DataView(message);
  const version = view.getUint8(4);
  if (version !== PACKET_VERSION) {
    return null;
  }

  const flags = view.getUint8(5);
  const frameId = readU64(view, 8);
  const width = view.getUint16(16, true);
  const height = view.getUint16(18, true);
  const rectCount = view.getUint16(20, true);
  const ptsUs = readU64(view, 24);

  let offset = VIDEO_PACKET_HEADER_SIZE;
  const rects: ParsedVncVideoRect[] = [];
  for (let index = 0; index < rectCount; index += 1) {
    if (offset + VIDEO_RECT_HEADER_SIZE > bytes.byteLength) {
      return null;
    }

    const x = view.getUint16(offset, true);
    const y = view.getUint16(offset + 2, true);
    const rectWidth = view.getUint16(offset + 4, true);
    const rectHeight = view.getUint16(offset + 6, true);
    const compression = view.getUint8(offset + 8);
    const rawSize = view.getUint32(offset + 12, true);
    const payloadSize = view.getUint32(offset + 16, true);
    offset += VIDEO_RECT_HEADER_SIZE;

    if (offset + payloadSize > bytes.byteLength) {
      return null;
    }
    const payload = bytes.slice(offset, offset + payloadSize);
    offset += payloadSize;

    const pixels = decompressPayload(compression, payload, rawSize);
    if (!pixels) {
      return null;
    }

    rects.push({ x, y, width: rectWidth, height: rectHeight, pixels });
  }

  return {
    frameId,
    width,
    height,
    ptsUs,
    frameBegin: (flags & 0b1) === 0b1,
    frameEnd: (flags & 0b10) === 0b10,
    rects,
  };
}

export function parseVncCursorPacket(message: ArrayBuffer): ParsedVncCursorPacket | null {
  const bytes = new Uint8Array(message);
  if (bytes.byteLength < CURSOR_PACKET_HEADER_SIZE || !hasMagic(bytes, CURSOR_PACKET_MAGIC)) {
    return null;
  }

  const view = new DataView(message);
  const version = view.getUint8(4);
  if (version !== PACKET_VERSION) {
    return null;
  }

  const kindId = view.getUint8(5);
  const x = view.getUint16(8, true);
  const y = view.getUint16(10, true);
  const hotspotX = view.getUint16(12, true);
  const hotspotY = view.getUint16(14, true);
  const width = view.getUint16(16, true);
  const height = view.getUint16(18, true);
  const payloadSize = view.getUint32(20, true);
  const compression = view.getUint8(24);

  if (CURSOR_PACKET_HEADER_SIZE + payloadSize > bytes.byteLength) {
    return null;
  }
  const payload = bytes.slice(CURSOR_PACKET_HEADER_SIZE, CURSOR_PACKET_HEADER_SIZE + payloadSize);

  const kind = kindId === 0 ? "default" : kindId === 1 ? "hidden" : kindId === 2 ? "position" : "bitmap";
  if (kind !== "bitmap") {
    return { kind, x, y, hotspotX, hotspotY, width, height };
  }

  const expectedSize = width > 0 && height > 0 ? width * height * 4 : 0;
  const bitmap = decompressPayload(compression, payload, expectedSize);
  if (!bitmap) {
    return null;
  }

  return { kind: "bitmap", x, y, hotspotX, hotspotY, width, height, bitmap };
}

export function emitVncVideoRects(detail: VncVideoRectsEventDetail): void {
  window.dispatchEvent(new CustomEvent<VncVideoRectsEventDetail>(VNC_VIDEO_RECTS_EVENT, { detail }));
}

export function onVncVideoRects(listener: (detail: VncVideoRectsEventDetail) => void): () => void {
  const handler = (event: Event) => {
    const custom = event as CustomEvent<VncVideoRectsEventDetail>;
    if (custom.detail) {
      listener(custom.detail);
    }
  };
  window.addEventListener(VNC_VIDEO_RECTS_EVENT, handler);
  return () => window.removeEventListener(VNC_VIDEO_RECTS_EVENT, handler);
}

export function emitVncCursor(detail: VncCursorEventDetail): void {
  window.dispatchEvent(new CustomEvent<VncCursorEventDetail>(VNC_CURSOR_EVENT, { detail }));
}

export function onVncCursor(listener: (detail: VncCursorEventDetail) => void): () => void {
  const handler = (event: Event) => {
    const custom = event as CustomEvent<VncCursorEventDetail>;
    if (custom.detail) {
      listener(custom.detail);
    }
  };
  window.addEventListener(VNC_CURSOR_EVENT, handler);
  return () => window.removeEventListener(VNC_CURSOR_EVENT, handler);
}
