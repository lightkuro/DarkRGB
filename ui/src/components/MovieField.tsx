// The setting that holds a moving picture for a matrix: a GIF, or a
// piece of video.
//
// A GIF is sent to the engine as it is. A video is played here, in the
// page, which already knows how to: each picture of it is shrunk to
// about as many dots as the largest matrix has and the lot is sent as
// plain frames. So the engine needs no video codecs, and any file this
// browser can play can be used.

import { Film, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { engine, useLibrary, useShow, type ParamSpec } from "../engine";
import { ConfirmButton } from "./ui";

/** The id of the moving picture that plays when none has been chosen. */
const BUILT_IN = "builtin";
/** Pictures a second taken from a video, and the most seconds of it. */
const VIDEO_RATE = 20;
const VIDEO_SECONDS = 60;
/** The most the frames of one moving picture may take up, as the engine has it. */
const MAX_BYTES = 24 * 1024 * 1024;
const MAX_FRAMES = 3600;

interface Frames {
  width: number;
  height: number;
  delays: number[];
  rgb: Uint8Array;
}

/** Reads a moving picture as the engine keeps it, or null. */
export function readMovie(bytes: Uint8Array): Frames | null {
  const mark = "DRGBMOV1";
  if (bytes.length < 17 || [...mark].some((c, i) => bytes[i] !== c.charCodeAt(0))) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let at = 8 + 1 + bytes[8]!;
  if (bytes.length < at + 8) return null;
  const width = view.getUint16(at, true);
  const height = view.getUint16(at + 2, true);
  const count = view.getUint32(at + 4, true);
  at += 8;
  if (bytes.length !== at + count * 2 + count * width * height * 3) return null;
  const delays = Array.from({ length: count }, (_, i) => view.getUint16(at + i * 2, true));
  return { width, height, delays, rgb: bytes.subarray(at + count * 2) };
}

/** Writes frames the way the engine reads them. The name goes separately. */
export function writeMovie(frames: Frames): Uint8Array {
  const count = frames.delays.length;
  const out = new Uint8Array(8 + 1 + 8 + count * 2 + frames.rgb.length);
  out.set([..."DRGBMOV1"].map((c) => c.charCodeAt(0)));
  const view = new DataView(out.buffer);
  view.setUint16(9, frames.width, true);
  view.setUint16(11, frames.height, true);
  view.setUint32(13, count, true);
  frames.delays.forEach((delay, i) => view.setUint16(17 + i * 2, delay, true));
  out.set(frames.rgb, 17 + count * 2);
  return out;
}

/** Waits for one event on a video, or gives up after a while. */
function once(video: HTMLVideoElement, event: string, ms = 8000): Promise<void> {
  return new Promise((resolve, reject) => {
    const done = (ok: boolean) => {
      clearTimeout(timer);
      video.removeEventListener(event, onEvent);
      video.removeEventListener("error", onError);
      if (ok) resolve();
      else reject(new Error("video"));
    };
    const onEvent = () => done(true);
    const onError = () => done(false);
    const timer = setTimeout(() => done(false), ms);
    video.addEventListener(event, onEvent);
    video.addEventListener("error", onError);
  });
}

/**
 * Plays a video file picture by picture and returns its frames, shrunk
 * to at most `side` dots along the longer side. The sound is left out.
 * `said` is told how far along it is, 0 to 1.
 */
export async function videoToFrames(file: Blob, side: number, said: (done: number) => void): Promise<Frames & { cut: boolean }> {
  const url = URL.createObjectURL(file);
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";
  try {
    video.src = url;
    await once(video, "loadeddata");
    // Some recordings do not say how long they are until their end has
    // been looked for.
    if (!Number.isFinite(video.duration)) {
      video.currentTime = 1e7;
      await once(video, "seeked").catch(() => undefined);
      video.currentTime = 0;
      await once(video, "seeked").catch(() => undefined);
    }
    const length = Number.isFinite(video.duration) && video.duration > 0 ? video.duration : 1 / VIDEO_RATE;
    const scale = Math.min(1, side / Math.max(video.videoWidth, video.videoHeight, 1));
    const width = Math.max(1, Math.round(video.videoWidth * scale));
    const height = Math.max(1, Math.round(video.videoHeight * scale));
    const wanted = Math.max(1, Math.floor(length * VIDEO_RATE));
    const room = Math.min(MAX_FRAMES, Math.floor(MAX_BYTES / (width * height * 3)), VIDEO_SECONDS * VIDEO_RATE);
    const count = Math.min(wanted, room);
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) throw new Error("no canvas");
    const rgb = new Uint8Array(count * width * height * 3);
    for (let i = 0; i < count; i += 1) {
      const t = i / VIDEO_RATE;
      // Asking for the place it is already at brings no answer.
      if (Math.abs(video.currentTime - t) > 1e-4) {
        video.currentTime = t;
        await once(video, "seeked");
      }
      ctx.drawImage(video, 0, 0, width, height);
      const rgba = ctx.getImageData(0, 0, width, height).data;
      let o = i * width * height * 3;
      for (let p = 0; p < rgba.length; p += 4) {
        rgb[o] = rgba[p]!;
        rgb[o + 1] = rgba[p + 1]!;
        rgb[o + 2] = rgba[p + 2]!;
        o += 3;
      }
      said((i + 1) / count);
    }
    return { width, height, delays: Array.from({ length: count }, () => 1000 / VIDEO_RATE), rgb, cut: count < wanted };
  } finally {
    video.removeAttribute("src");
    video.load();
    URL.revokeObjectURL(url);
  }
}

/** A small copy of a kept moving picture, playing. */
function MoviePreview({ id }: { id: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void (async () => {
      try {
        const response = await fetch(`/api/movie/${encodeURIComponent(id)}`);
        if (!response.ok) return;
        const movie = readMovie(new Uint8Array(await response.arrayBuffer()));
        const el = canvas.current;
        const ctx = el?.getContext("2d");
        if (!movie || !el || !ctx || stopped) return;
        el.width = movie.width;
        el.height = movie.height;
        const image = ctx.createImageData(movie.width, movie.height);
        const dots = movie.width * movie.height;
        let frame = 0;
        const draw = () => {
          if (stopped) return;
          const from = frame * dots * 3;
          for (let i = 0; i < dots; i += 1) {
            image.data[i * 4] = movie.rgb[from + i * 3]!;
            image.data[i * 4 + 1] = movie.rgb[from + i * 3 + 1]!;
            image.data[i * 4 + 2] = movie.rgb[from + i * 3 + 2]!;
            image.data[i * 4 + 3] = 255;
          }
          ctx.putImageData(image, 0, 0);
          const wait = movie.delays[frame] ?? 100;
          frame = (frame + 1) % movie.delays.length;
          // A still has nothing to wait for.
          if (movie.delays.length > 1) timer = setTimeout(draw, wait);
        };
        draw();
      } catch {
        // No preview is no harm: the stage shows it too.
      }
    })();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [id]);
  return <canvas ref={canvas} className="picture-preview" aria-label="The moving picture as the lights get it" />;
}

export function MovieField({ spec, value, layer }: { spec: ParamSpec; value: string; layer: number }) {
  const show = useShow();
  const library = useLibrary();
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const movies = library.movies ?? [];
  const current = movies.find((m) => m.id === value);
  const missing = value !== BUILT_IN && !current;

  // As many dots as the largest matrix has along its longer side; more
  // would be thrown away by the lights anyway.
  const largest = Math.max(
    0,
    ...(show?.fixtures ?? []).map((f) => (f.shape.type === "matrix" ? Math.max(f.shape.cols, f.shape.rows) : 0)),
  );
  const side = Math.min(64, largest || 32);
  const use = (id: string) => engine.send({ type: "set_param", layer, param: spec.id, value: id });

  const choose = async (file: File | undefined) => {
    if (!file || busy) return;
    setError(null);
    setNote(null);
    try {
      const gif = file.type === "image/gif" || /\.gif$/i.test(file.name);
      let result;
      if (gif) {
        setBusy("Reading the GIF…");
        result = await engine.uploadMovie(file.name, "gif", file);
      } else {
        setBusy("Reading the video…");
        const frames = await videoToFrames(file, side, (done) => setBusy(`Reading the video: ${Math.round(done * 100)}%`));
        if (frames.cut) setNote(`Only the first ${Math.round(frames.delays.length / VIDEO_RATE)} seconds were kept.`);
        setBusy("Sending it to the lights…");
        result = await engine.uploadMovie(file.name, "frames", writeMovie(frames));
      }
      if (result.ok && result.movie) use(result.movie.id);
      else setError(result.error ?? "That file could not be kept.");
    } catch {
      setError(
        "That file could not be played here. GIFs always work; for video, MP4 and WebM work in most browsers.",
      );
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="field">
      <span className="field-head">
        <span>{spec.label}</span>
        <span className="value">
          {current
            ? `${current.width} × ${current.height} dots, ${current.seconds < 10 ? current.seconds.toFixed(1) : Math.round(current.seconds)} s`
            : missing
              ? "Not on this computer"
              : "A bouncing ball"}
        </span>
      </span>
      <div className="picture-field">
        <MoviePreview id={current ? value : BUILT_IN} />
        <div className="movie-actions">
          <button type="button" className="button small" disabled={busy !== null} onClick={() => input.current?.click()}>
            <Film size={14} /> {busy ?? "Choose a GIF or video…"}
          </button>
          {movies.length > 0 && (
            <select
              aria-label="Moving pictures you have"
              value={current ? value : ""}
              onChange={(e) => use(e.target.value || BUILT_IN)}
            >
              <option value="">{missing ? "Missing: choose another" : "The bouncing ball"}</option>
              {movies.map((movie) => (
                <option key={movie.id} value={movie.id}>
                  {movie.name}
                </option>
              ))}
            </select>
          )}
        </div>
        {current && (
          <ConfirmButton
            label={`Delete ${current.name} from this computer`}
            confirmLabel="Delete?"
            onConfirm={() => {
              engine.send({ type: "remove_movie", id: current.id });
              use(BUILT_IN);
            }}
          >
            <Trash2 size={15} />
          </ConfirmButton>
        )}
        <input
          ref={input}
          type="file"
          accept="image/gif,video/*,.gif,.mp4,.webm,.mov,.m4v"
          hidden
          aria-label="GIF or video file"
          onChange={(e) => {
            void choose(e.target.files?.[0]);
            // So the same file can be chosen again after a change.
            e.target.value = "";
          }}
        />
      </div>
      <p className="hint">
        {missing
          ? "This show uses a moving picture that is not kept on this computer, so a dim stand-in plays. Choose it again here. "
          : ""}
        It is shrunk to suit your lights: a video to at most {side} dots a side, a GIF to at most 64. Of a video, up
        to the first minute is kept, without its sound.
      </p>
      {note && <p className="hint">{note}</p>}
      {error && <p className="callout error">{error}</p>}
    </div>
  );
}
