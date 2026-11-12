// Plays the timeline's song in this window and keeps the engine in step
// with it.
//
// The engine owns the timeline's clock. But sound is what people notice
// being late, so while this window plays the song it tells the engine,
// four times a second, where the song really is, and the lights follow.
// A jump the engine makes (someone moved the playhead, or playback went
// round again) is the other way about: the song follows.
//
// The song comes from the engine, which keeps the file. It is fetched
// whole, so that jumping about in it needs nothing more from the engine,
// and decoded once more to draw its outline and to guess its tempo.

import { useSyncExternalStore } from "react";
import { engine, Store } from "./engine";

export interface SongInfo {
  /** The file the engine names it by, to tell songs apart. */
  file: string;
  state: "loading" | "ready" | "failed";
  /** Seconds, once known. */
  duration: number;
  /** The loudest sample in each of many equal slices, 0..1, for drawing. */
  peaks: Float32Array | null;
  /** A guess at the tempo and where its first beat falls, if one could be made. */
  tempo: { bpm: number; offset: number } | null;
  /** This device may not play sound until it is touched. */
  blocked: boolean;
}

export const song = new Store<SongInfo | null>(null);
export const useSong = () => useSyncExternalStore(song.subscribe, song.get);

const audio = typeof Audio === "undefined" ? null : new Audio();
let objectUrl: string | null = null;
let loaded: string | null = null;

/** How far the song and the engine may differ before the song is moved. */
const JUMP_S = 0.25;

/**
 * Guesses a tempo from the sound itself: how strongly loudness rises
 * repeat at each spacing between 70 and 180 beats a minute, and where
 * the beats then fall. Good enough to start a grid from; the tempo can
 * be halved, doubled or typed afterwards.
 */
export function guessTempo(samples: Float32Array, rate: number): { bpm: number; offset: number } | null {
  const hop = 512;
  const frames = Math.floor(samples.length / hop);
  if (frames < 400) return null;
  // How much louder each slice is than the one before.
  const rise = new Float32Array(frames);
  let before = 0;
  for (let f = 0; f < frames; f++) {
    let sum = 0;
    for (let i = f * hop, end = i + hop; i < end; i++) sum += samples[i]! * samples[i]!;
    const level = Math.log1p(Math.sqrt(sum / hop) * 100);
    rise[f] = Math.max(0, level - before);
    before = level;
  }
  const perSecond = rate / hop;
  // The first two minutes say enough, and keep this quick.
  const span = Math.min(frames, Math.floor(perSecond * 120));
  let best = { lag: 0, score: 0 };
  for (let bpm = 70; bpm <= 180; bpm += 0.5) {
    const lag = (60 / bpm) * perSecond;
    let score = 0;
    for (let f = 0; f + lag * 2 < span - 1; f++) {
      // Between slices, so tempos that fall between whole slices count.
      const at = f + lag;
      const low = Math.floor(at);
      const here = rise[low]! + (rise[low + 1]! - rise[low]!) * (at - low);
      const at2 = f + lag * 2;
      const low2 = Math.floor(at2);
      const there = rise[low2]! + (rise[low2 + 1]! - rise[low2]!) * (at2 - low2);
      score += rise[f]! * (here + 0.5 * there);
    }
    // A slight lean towards the middle of the range, to break ties
    // between a tempo and its double.
    score *= 1 - 0.15 * Math.abs(Math.log2(bpm / 120));
    if (score > best.score) best = { lag, score };
  }
  if (best.score <= 0) return null;
  // Where in each beat the rises pile up.
  const steps = Math.max(1, Math.floor(best.lag));
  let phase = { at: 0, sum: -1 };
  for (let o = 0; o < steps; o++) {
    let sum = 0;
    for (let f = o; f < span; f += best.lag) sum += rise[Math.floor(f)]!;
    if (sum > phase.sum) phase = { at: o, sum };
  }
  const bpm = Math.round((60 * perSecond) / best.lag * 2) / 2;
  return { bpm, offset: Math.round((phase.at / perSecond) * 1000) / 1000 };
}

/** The loudest sample in each of `count` equal slices. */
function outline(samples: Float32Array, count: number): Float32Array {
  const peaks = new Float32Array(count);
  const size = samples.length / count;
  for (let p = 0; p < count; p++) {
    let top = 0;
    for (let i = Math.floor(p * size), end = Math.min(samples.length, Math.floor((p + 1) * size)); i < end; i++) {
      const v = Math.abs(samples[i]!);
      if (v > top) top = v;
    }
    peaks[p] = top;
  }
  return peaks;
}

async function load(file: string): Promise<void> {
  if (!audio) return;
  loaded = file;
  song.set({ file, state: "loading", duration: 0, peaks: null, tempo: null, blocked: false });
  try {
    const response = await fetch(`/api/song/${encodeURIComponent(file)}`);
    if (!response.ok) throw new Error(String(response.status));
    const bytes = await response.arrayBuffer();
    if (loaded !== file) return;
    if (objectUrl) URL.revokeObjectURL(objectUrl);
    objectUrl = URL.createObjectURL(new Blob([bytes], { type: response.headers.get("Content-Type") ?? "audio/mpeg" }));
    audio.src = objectUrl;
    audio.preload = "auto";
    await new Promise<void>((resolve, reject) => {
      audio.onloadedmetadata = () => resolve();
      audio.onerror = () => reject(new Error("cannot play"));
    });
    if (loaded !== file) return;
    const duration = Number.isFinite(audio.duration) ? audio.duration : 0;
    song.set({ file, state: "ready", duration, peaks: null, tempo: null, blocked: false });
    // The outline and the tempo are a nicety: the song plays without them.
    try {
      const Context = window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      const context = new Context();
      const decoded = await context.decodeAudioData(bytes.slice(0));
      void context.close();
      if (loaded !== file) return;
      const mono = new Float32Array(decoded.length);
      for (let c = 0; c < decoded.numberOfChannels; c++) {
        const channel = decoded.getChannelData(c);
        for (let i = 0; i < mono.length; i++) mono[i]! += channel[i]! / decoded.numberOfChannels;
      }
      const peaks = outline(mono, Math.min(12000, Math.max(200, Math.round(decoded.duration * 60))));
      const now = song.get();
      if (now?.file === file) {
        song.set({ ...now, duration: duration || decoded.duration, peaks, tempo: guessTempo(mono, decoded.sampleRate) });
      }
    } catch {
      // No outline. Some systems cannot decode what they can play.
    }
  } catch {
    if (loaded === file) song.set({ file, state: "failed", duration: 0, peaks: null, tempo: null, blocked: false });
  }
}

function unload(): void {
  loaded = null;
  if (audio) {
    audio.pause();
    audio.removeAttribute("src");
  }
  if (objectUrl) URL.revokeObjectURL(objectUrl);
  objectUrl = null;
  song.set(null);
}

/** Moves the song to where the engine is. For jumps made in this window. */
export function jumpSong(position: number): void {
  if (audio && song.get()?.state === "ready") audio.currentTime = Math.max(0, Math.min(position, audio.duration || position));
}

/** Once a tick: start, stop, follow a jump, or say where the song is. */
function follow(): void {
  const info = song.get();
  if (!audio || info?.state !== "ready") return;
  const playing = engine.live.get().timeline?.playing ?? false;
  if (!playing) {
    if (!audio.paused) audio.pause();
    return;
  }
  const at = engine.playhead();
  // Past the end of the song the timeline plays on in silence.
  if (at >= info.duration - 0.05) {
    if (!audio.paused) audio.pause();
    return;
  }
  if (audio.paused) {
    audio.currentTime = at;
    audio.play().then(
      () => info.blocked && song.set({ ...info, blocked: false }),
      () => !info.blocked && song.set({ ...info, blocked: true }),
    );
    return;
  }
  if (Math.abs(audio.currentTime - at) > JUMP_S) {
    // The engine jumped: the song follows.
    audio.currentTime = at;
  } else {
    // Otherwise the song is the truth, and the engine is told.
    engine.quiet({ type: "timeline_sync", position: audio.currentTime });
  }
}

/**
 * Starts looking after the show's song: loads it when the show names
 * one, drops it when it does not, and follows the engine's playback.
 * Only on the engine's own computer: a phone used as a remote must not
 * start playing the song itself.
 */
export function startSongPlayer(): () => void {
  const sync = () => {
    const remote = engine.status.get()?.remote;
    const here = remote ? remote.local : true;
    const file = here ? (engine.show.get()?.timeline?.song?.file ?? null) : null;
    if (file === loaded) return;
    if (file) void load(file);
    else unload();
  };
  const unsubscribe = [engine.show.subscribe(sync), engine.status.subscribe(sync), engine.live.subscribe(follow)];
  const timer = window.setInterval(follow, 250);
  sync();
  return () => {
    unsubscribe.forEach((stop) => stop());
    window.clearInterval(timer);
    unload();
  };
}
