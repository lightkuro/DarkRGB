// Shares this computer's screen with the engine, for the "Screen colours"
// effect. The browser (or the app's window) asks which screen or window
// to share; each picture is shrunk to a few dozen pixels across and sent
// about fifteen times a second. Nothing is recorded or kept: the engine
// holds only the newest small picture, and forgets it when sharing stops.

import { engine, Store } from "./engine";

/** Pixels across the picture sent. Its height keeps the screen's shape. */
const WIDTH = 64;
const EVERY_MS = 66;

export type ShareState = { on: false; error: string | null } | { on: true; label: string };

export const screenShare = new Store<ShareState>({ on: false, error: null });

let stream: MediaStream | null = null;
let timer: number | null = null;

/** Whether this window can be asked to share a screen at all. */
export function canShare(): boolean {
  return typeof navigator !== "undefined" && !!navigator.mediaDevices?.getDisplayMedia;
}

function hex(data: Uint8ClampedArray): string {
  const out = new Array<string>(data.length / 4);
  for (let i = 0, p = 0; i < data.length; i += 4, p++) {
    const r = data[i] ?? 0;
    const g = data[i + 1] ?? 0;
    const b = data[i + 2] ?? 0;
    out[p] = ((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1);
  }
  return out.join("");
}

export async function startSharing(): Promise<void> {
  if (stream) return;
  if (!canShare()) {
    screenShare.set({
      on: false,
      error:
        "This window cannot share the screen. Open darkRGB in a browser on this computer (Chrome, Edge or Firefox) and share it from there.",
    });
    return;
  }
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({ video: { frameRate: 15 }, audio: false });
  } catch (e) {
    const denied = e instanceof DOMException && e.name === "NotAllowedError";
    screenShare.set({ on: false, error: denied ? null : `The screen could not be shared: ${String(e)}` });
    return;
  }
  const track = stream.getVideoTracks()[0];
  if (!track) {
    stopSharing();
    return;
  }
  track.addEventListener("ended", () => stopSharing());
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;
  await video.play().catch(() => undefined);
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    stopSharing();
    return;
  }
  // The browser's own label reads "screen:1:0" and the like: say what
  // kind of thing is shared instead.
  const surface = (track.getSettings() as { displaySurface?: string }).displaySurface;
  const label = surface === "window" ? "a window" : surface === "browser" ? "a browser tab" : "the whole screen";
  screenShare.set({ on: true, label });
  timer = window.setInterval(() => {
    const [w, h] = [video.videoWidth, video.videoHeight];
    if (!w || !h) return;
    const height = Math.max(1, Math.min(WIDTH, Math.round((WIDTH * h) / w)));
    if (canvas.width !== WIDTH || canvas.height !== height) {
      canvas.width = WIDTH;
      canvas.height = height;
    }
    // Drawn small, the browser averages the colours as it shrinks it.
    context.drawImage(video, 0, 0, WIDTH, height);
    const pixels = context.getImageData(0, 0, WIDTH, height).data;
    engine.quiet({ type: "screen_frame", width: WIDTH, height, rgb: hex(pixels) });
  }, EVERY_MS);
}

export function stopSharing(): void {
  if (timer !== null) window.clearInterval(timer);
  timer = null;
  const was = stream;
  stream = null;
  for (const track of was?.getTracks() ?? []) track.stop();
  if (was) engine.send({ type: "screen_stop" });
  screenShare.set({ on: false, error: null });
}
