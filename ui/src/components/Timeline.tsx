// The timeline: a show laid out in time against a song.
//
// Scenes have one lane: a scene clip says "from here, this scene". Every
// pad has a lane of its own: a clip there holds the pad from its start to
// its end. Clips are added by clicking or dragging in a lane, moved and
// stretched by dragging them, and removed with Delete. With Record on,
// whatever is played while the timeline runs is written down as clips.

import { Circle, Minus, Music, Plus, Repeat, SkipBack, Square, Trash2, Play as PlayIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import {
  engine,
  NO_TIMELINE,
  perFrame,
  Pref,
  useHello,
  useLive,
  useShow,
  type Binding,
  type Clip,
  type Show,
  type Timeline,
} from "../engine";
import { jumpSong, useSong } from "../songPlayer";
import { describe } from "./LivePads";
import { ConfirmButton, Switch } from "./ui";

const zoomPref = new Pref<number>("darkrgb.timeline.zoom", 24);
const snapPref = new Pref<number>("darkrgb.timeline.snap", 1);
const ZOOMS = [4, 8, 12, 24, 48, 96, 192];
const ROW = 34;

/** Minutes, seconds and tenths. */
export function clockTime(seconds: number): string {
  const s = Math.max(0, seconds);
  const whole = Math.floor(s);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}.${Math.floor((s - whole) * 10)}`;
}

/** Pads that can be held by a clip: everything but scenes and things done once. */
function lanePads(show: Show): Binding[] {
  return show.bindings.filter((b) => !["scene", "tap_tempo", "next_scene", "prev_scene"].includes(b.action.type));
}

function seek(position: number): void {
  engine.send({ type: "timeline_seek", position });
  jumpSong(position);
}

/** Play, stop, record and the time. Also shown in the Live view, where a show is performed. */
export function Transport({ compact }: { compact?: boolean }) {
  const live = useLive();
  const show = useShow();
  const time = useRef<HTMLSpanElement>(null);
  const playing = live.timeline?.playing ?? false;
  const recording = live.timeline?.recording ?? false;
  const length = show?.timeline?.length ?? NO_TIMELINE.length;

  useEffect(() => {
    let frame = 0;
    const tick = () => {
      if (time.current) time.current.textContent = clockTime(Math.min(engine.playhead(), length));
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [length]);

  return (
    <div className={`transport ${compact ? "compact" : ""}`} role="group" aria-label="Timeline playback">
      <button type="button" className="tool" title="Back to the start" aria-label="Back to the start" onClick={() => seek(0)}>
        <SkipBack size={15} />
      </button>
      <button
        type="button"
        className={`tool play ${playing ? "active" : ""}`}
        aria-pressed={playing}
        title={playing ? "Stop" : "Play from here"}
        onClick={() => {
          if (playing) return engine.send({ type: "timeline_stop" });
          // Started from this window, so the song may start with it.
          jumpSong(engine.playhead() >= length - 0.01 ? 0 : engine.playhead());
          engine.send({ type: "timeline_play" });
        }}
      >
        {playing ? <Square size={14} /> : <PlayIcon size={15} />} {playing ? "Stop" : "Play"}
      </button>
      <button
        type="button"
        className={`tool record ${recording ? "active" : ""}`}
        aria-pressed={recording}
        title="While the timeline plays, write down the scenes you choose and the pads you press"
        onClick={() => engine.send({ type: "timeline_record", on: !recording })}
      >
        <Circle size={13} fill={recording ? "currentColor" : "none"} /> Record
      </button>
      <span className="transport-time" aria-label="Position">
        <span ref={time}>0:00.0</span>
        <span className="of"> / {clockTime(length)}</span>
      </span>
    </div>
  );
}

type Drag =
  | { kind: "seek" }
  | { kind: "create"; pad: number; from: number }
  | { kind: "move"; grabbed: number; at: number; starts: Map<number, number> }
  | { kind: "stretch"; id: number; start: number };

const sendClips = perFrame((clips: { id: number; start?: number; length?: number }[]) =>
  engine.quiet({ type: "update_clips", clips }),
);
const sendSeek = perFrame((position: number) => seek(position));

/** A number that can be typed, sent when the field is left. */
function NumberBox({ value, label, width, onCommit }: { value: string; label: string; width: number; onCommit: (text: string) => void }) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <input
      className="number"
      style={{ width }}
      type="text"
      inputMode="decimal"
      aria-label={label}
      title={label}
      value={draft ?? value}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft !== null && draft.trim() !== "" && draft !== value) onCommit(draft.trim());
        setDraft(null);
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") setDraft(null);
      }}
    />
  );
}

function Waveform({ peaks, seconds, pps }: { peaks: Float32Array; seconds: number; pps: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const width = Math.max(1, Math.round(seconds * pps));
  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;
    // Browsers cap how wide a canvas may be; it is stretched past that.
    el.width = Math.min(16000, width);
    el.height = ROW * 2;
    ctx.clearRect(0, 0, el.width, el.height);
    ctx.fillStyle = "#4a8f8a";
    const mid = el.height / 2;
    for (let x = 0; x < el.width; x++) {
      const from = Math.floor((x / el.width) * peaks.length);
      const to = Math.max(from + 1, Math.floor(((x + 1) / el.width) * peaks.length));
      let top = 0;
      for (let i = from; i < to && i < peaks.length; i++) top = Math.max(top, peaks[i]!);
      const h = Math.max(1, top * (el.height - 4));
      ctx.fillRect(x, mid - h / 2, 1, h);
    }
  }, [peaks, width]);
  return <canvas ref={canvas} className="wave" style={{ width }} aria-hidden />;
}

export function TimelineView() {
  const show = useShow();
  const hello = useHello();
  const info = useSong();
  const pps = useSyncExternalStore(zoomPref.subscribe, zoomPref.get);
  const snap = useSyncExternalStore(snapPref.subscribe, snapPref.get);
  const [picked, setPicked] = useState<number[]>([]);
  // Where clips are while they are being dragged, before the engine answers.
  const [drafts, setDrafts] = useState<Map<number, { start: number; length: number }>>(new Map());
  const [ghost, setGhost] = useState<{ pad: number; start: number; length: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; at: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const lanes = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const head = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  // The song just chosen here, whose length the timeline should take.
  const fitTo = useRef<string | null>(null);

  const timeline: Timeline = show?.timeline ?? NO_TIMELINE;
  const pads = useMemo(() => (show ? lanePads(show) : []), [show]);
  const width = Math.round(timeline.length * pps);
  const beat = timeline.bpm > 0 ? 60 / timeline.bpm : 0;

  const snapped = (t: number): number => {
    if (!snap || !beat) return Math.max(0, t);
    const step = beat / snap;
    return Math.max(0, Math.round((t - timeline.offset) / step) * step + timeline.offset);
  };
  const timeAt = (e: { clientX: number }): number => {
    const box = lanes.current!.getBoundingClientRect();
    return Math.min(timeline.length, Math.max(0, (e.clientX - box.left) / pps));
  };
  const placed = (clip: Clip) => drafts.get(clip.id) ?? clip;

  // The playhead, moved every frame without redrawing anything else.
  useEffect(() => {
    let frame = 0;
    const tick = () => {
      const at = Math.min(engine.playhead(), timeline.length);
      if (head.current) head.current.style.transform = `translateX(${at * pps}px)`;
      // Keep it in sight while it plays.
      const view = scroller.current;
      if (view && engine.live.get().timeline?.playing && !drag.current) {
        const x = at * pps;
        if (x < view.scrollLeft || x > view.scrollLeft + view.clientWidth - 40) view.scrollLeft = Math.max(0, x - 80);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [pps, timeline.length]);

  // A song chosen here gives the timeline its length, and a first guess at its beat.
  useEffect(() => {
    if (!info || info.state !== "ready" || fitTo.current !== info.file || !info.peaks) return;
    fitTo.current = null;
    engine.send({
      type: "set_timeline",
      length: Math.max(1, Math.round(info.duration * 100) / 100),
      ...(timeline.bpm === 0 && info.tempo ? { bpm: info.tempo.bpm, offset: info.tempo.offset } : {}),
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info]);

  // Delete takes the picked clips away.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && (e.target.tagName === "INPUT" || e.target.tagName === "SELECT");
      if (typing || picked.length === 0) return;
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        engine.send({ type: "remove_clips", ids: picked });
        setPicked([]);
      }
      if (e.key === "Escape") setPicked([]);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [picked]);

  if (!show || !hello) return null;

  const scenes = timeline.clips.filter((c) => c.what.type === "scene").sort((a, b) => placed(a).start - placed(b).start);
  const sceneName = (id: number) => show.scenes.find((s) => s.id === id)?.name ?? "?";

  const begin = (e: React.PointerEvent, next: Drag) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    drag.current = next;
    lanes.current!.setPointerCapture(e.pointerId);
  };
  const onMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const t = timeAt(e);
    if (d.kind === "seek") return sendSeek(t);
    if (d.kind === "create") {
      const [a, b] = [snapped(Math.min(d.from, t)), snapped(Math.max(d.from, t))];
      return setGhost({ pad: d.pad, start: a, length: Math.max(0, b - a) });
    }
    if (d.kind === "stretch") {
      const clip = timeline.clips.find((c) => c.id === d.id);
      if (!clip) return;
      const length = Math.max(0.02, snapped(t) - d.start);
      setDrafts(new Map([[d.id, { start: d.start, length }]]));
      return sendClips([{ id: d.id, length }]);
    }
    // Moving: the grabbed clip snaps, and the others keep their distance from it.
    const grabbedStart = d.starts.get(d.grabbed)!;
    let by = snapped(grabbedStart + (t - d.at)) - grabbedStart;
    for (const start of d.starts.values()) by = Math.max(by, -start);
    const next = new Map<number, { start: number; length: number }>();
    for (const [id, start] of d.starts) {
      const clip = timeline.clips.find((c) => c.id === id);
      if (clip) next.set(id, { start: Math.min(timeline.length, start + by), length: clip.length });
    }
    setDrafts(next);
    sendClips([...next].map(([id, p]) => ({ id, start: p.start })));
  };
  const onUp = () => {
    const d = drag.current;
    drag.current = null;
    if (d?.kind === "create" && ghost) {
      // A click with no drag makes a clip one beat long, or a quarter of a second.
      const length = ghost.length >= 0.03 ? ghost.length : beat || 0.25;
      engine.send({ type: "add_clip", start: ghost.start, length, what: { type: "pad", id: d.pad } });
    }
    setGhost(null);
    // Leave the drafts a moment, until the engine's answer is in.
    window.setTimeout(() => setDrafts(new Map()), 120);
  };

  const pickClip = (e: React.PointerEvent, clip: Clip): number[] => {
    if (e.shiftKey) {
      const next = picked.includes(clip.id) ? picked.filter((id) => id !== clip.id) : [...picked, clip.id];
      setPicked(next);
      return next;
    }
    const next = picked.includes(clip.id) ? picked : [clip.id];
    setPicked(next);
    return next;
  };
  const grab = (e: React.PointerEvent, clip: Clip) => {
    const ids = pickClip(e, clip);
    if (e.shiftKey) return e.stopPropagation();
    const starts = new Map(timeline.clips.filter((c) => ids.includes(c.id)).map((c) => [c.id, c.start]));
    begin(e, { kind: "move", grabbed: clip.id, at: timeAt(e), starts });
  };

  const chooseSong = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    const result = await engine.uploadSong(file);
    setBusy(false);
    if (!result.ok) return engine.toast("error", result.error ?? "Could not use that file.");
    fitTo.current = "pending";
  };
  // The engine names the song by what is in it; take that name once it is known.
  if (fitTo.current === "pending" && timeline.song) fitTo.current = timeline.song.file;

  // Time marks far enough apart to read.
  const every = [1, 2, 5, 10, 15, 30, 60, 120, 300].find((s) => s * pps >= 64) ?? 600;
  const marks = Array.from({ length: Math.floor(timeline.length / every) + 1 }, (_, i) => i * every);
  const grid =
    beat * pps >= 5
      ? {
          backgroundImage: `linear-gradient(90deg, var(--grid-bar) 1px, transparent 1px), linear-gradient(90deg, var(--grid-beat) 1px, transparent 1px)`,
          backgroundSize: `${beat * 4 * pps}px 100%, ${beat * pps}px 100%`,
          backgroundPosition: `${timeline.offset * pps}px 0`,
        }
      : undefined;
  const zoom = (by: number) => {
    const at = ZOOMS.indexOf(ZOOMS.reduce((a, b) => (Math.abs(b - pps) < Math.abs(a - pps) ? b : a)));
    zoomPref.set(ZOOMS[Math.min(ZOOMS.length - 1, Math.max(0, at + by))]!);
  };

  return (
    <section className="timeline" aria-label="Timeline">
      <div className="timeline-bar">
        <Transport />
        <div className="tool-group">
          <button type="button" className="tool" disabled={busy} onClick={() => fileInput.current?.click()} title="The song this timeline plays against">
            <Music size={14} /> {busy ? "Taking the song…" : timeline.song ? timeline.song.name : "Choose a song…"}
          </button>
          <input ref={fileInput} type="file" accept="audio/*,.mp3,.wav,.ogg,.flac,.m4a,.aac,.opus" hidden aria-label="Song file" onChange={(e) => { void chooseSong(e.target.files?.[0]); e.target.value = ""; }} />
          {timeline.song && (
            <button type="button" className="tool" aria-label="Let go of the song" title="Let go of the song. The clips stay." onClick={() => engine.send({ type: "remove_song" })}>
              <Trash2 size={14} />
            </button>
          )}
        </div>
        {!timeline.song && (
          <label className="tool-field">
            <span>Seconds</span>
            <NumberBox value={String(Math.round(timeline.length))} label="Length in seconds" width={56} onCommit={(text) => engine.send({ type: "set_timeline", length: Number(text) })} />
          </label>
        )}
        <label className="tool-field">
          <span>Tempo</span>
          <NumberBox value={timeline.bpm ? String(Math.round(timeline.bpm * 10) / 10) : ""} label="Beats a minute. Empty for no grid." width={52} onCommit={(text) => engine.send({ type: "set_timeline", bpm: Number(text) || 0 })} />
        </label>
        {timeline.bpm > 0 && (
          <div className="tool-group">
            <button type="button" className="tool" title="Half the tempo" onClick={() => engine.send({ type: "set_timeline", bpm: timeline.bpm / 2 })}>÷2</button>
            <button type="button" className="tool" title="Double the tempo" onClick={() => engine.send({ type: "set_timeline", bpm: timeline.bpm * 2 })}>×2</button>
            <button type="button" className="tool" title="Put the first beat of the grid where the playhead is" onClick={() => engine.send({ type: "set_timeline", offset: beat ? engine.playhead() % beat : 0 })}>
              Beat here
            </button>
          </div>
        )}
        {info?.tempo && info.tempo.bpm !== timeline.bpm && (
          <button type="button" className="tool" title="Take the tempo worked out from the song" onClick={() => engine.send({ type: "set_timeline", bpm: info.tempo!.bpm, offset: info.tempo!.offset })}>
            Use {info.tempo.bpm} from the song
          </button>
        )}
        <label className="tool-field">
          <span>Snap</span>
          <select value={snap} aria-label="Snap clips to" onChange={(e) => snapPref.set(Number(e.target.value))} disabled={!beat}>
            <option value={0}>Off</option>
            <option value={1}>Beats</option>
            <option value={2}>Half beats</option>
            <option value={4}>Quarter beats</option>
          </select>
        </label>
        <label className="tool-field" title="Start again from the beginning on reaching the end">
          <Repeat size={13} aria-hidden />
          <Switch checked={timeline.repeat} label="Repeat" onChange={(repeat) => engine.send({ type: "set_timeline", repeat })} />
        </label>
        <div className="tool-group">
          <button type="button" className="tool" aria-label="Zoom out" onClick={() => zoom(-1)}><Minus size={14} /></button>
          <button type="button" className="tool" aria-label="Zoom in" onClick={() => zoom(1)}><Plus size={14} /></button>
        </div>
        {timeline.clips.length > 0 && (
          <ConfirmButton label="Take every clip off the timeline" confirmLabel="Clear all?" onConfirm={() => engine.send({ type: "clear_timeline" })}>
            <Trash2 size={14} />
          </ConfirmButton>
        )}
      </div>
      {info?.blocked && <p className="callout">Click anywhere in this window to let the song play.</p>}
      {info?.state === "failed" && <p className="callout error">This computer could not play that song file. The timeline still runs, in silence. WAV, OGG and FLAC files work everywhere.</p>}

      <div className="timeline-body">
        <div className="lane-names" style={{ paddingTop: ROW + (timeline.song ? ROW * 2 : 0) }}>
          <div className="lane-name">Scenes</div>
          {pads.map((pad) => {
            const what = describe(pad.action, show, hello);
            return (
              <div key={pad.id} className="lane-name" title={what.name}>
                <span className="lane-dot" style={{ background: what.colour ?? "var(--muted)" }} />
                {what.name}
              </div>
            );
          })}
        </div>
        <div className="lane-scroll" ref={scroller}>
          <div className="lanes" ref={lanes} data-pps={pps} style={{ width, ...grid }} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
            <div className="ruler" onPointerDown={(e) => { begin(e, { kind: "seek" }); seek(timeAt(e)); }}>
              {marks.map((s) => (
                <span key={s} style={{ left: s * pps }}>{clockTime(s).slice(0, -2)}</span>
              ))}
            </div>
            {timeline.song && (
              <div className="lane wave-lane" style={{ height: ROW * 2 }} onPointerDown={(e) => { begin(e, { kind: "seek" }); seek(timeAt(e)); }}>
                {info?.peaks ? <Waveform peaks={info.peaks} seconds={Math.min(info.duration, timeline.length)} pps={pps} /> : <span className="lane-hint">{info?.state === "loading" ? "Loading the song…" : ""}</span>}
              </div>
            )}
            <div
              className="lane scene-lane"
              onPointerDown={(e) => {
                if (e.button !== 0) return;
                const box = e.currentTarget.getBoundingClientRect();
                setPicked([]);
                setMenu({ x: e.clientX - box.left, y: 0, at: snapped(timeAt(e)) });
              }}
            >
              {scenes.length === 0 && <span className="lane-hint">Click to say which scene starts here</span>}
              {scenes.map((clip, i) => {
                const at = placed(clip).start;
                const until = i + 1 < scenes.length ? placed(scenes[i + 1]!).start : timeline.length;
                return (
                  <div
                    key={clip.id}
                    className={`clip scene-clip ${picked.includes(clip.id) ? "picked" : ""}`}
                    style={{ left: at * pps, width: Math.max(8, (until - at) * pps - 1) }}
                    title={`${sceneName(clip.what.id)}, from ${clockTime(at)}`}
                    onPointerDown={(e) => grab(e, clip)}
                  >
                    {sceneName(clip.what.id)}
                  </div>
                );
              })}
              {menu && (
                <div className="clip-menu" style={{ left: Math.min(menu.x, width - 160) }} role="menu" onPointerDown={(e) => e.stopPropagation()}>
                  {show.scenes.length === 0 && <p className="hint">Save a scene first, in the Edit view.</p>}
                  {show.scenes.map((scene) => (
                    <button
                      key={scene.id}
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        engine.send({ type: "add_clip", start: menu.at, what: { type: "scene", id: scene.id } });
                        setMenu(null);
                      }}
                    >
                      {scene.name}
                    </button>
                  ))}
                  <button type="button" role="menuitem" className="quiet" onClick={() => setMenu(null)}>Cancel</button>
                </div>
              )}
            </div>
            {pads.map((pad) => {
              const what = describe(pad.action, show, hello);
              return (
                <div
                  key={pad.id}
                  className="lane"
                  onPointerDown={(e) => {
                    setMenu(null);
                    setPicked([]);
                    const from = timeAt(e);
                    setGhost({ pad: pad.id, start: snapped(from), length: 0 });
                    begin(e, { kind: "create", pad: pad.id, from });
                  }}
                >
                  {timeline.clips
                    .filter((c) => c.what.type === "pad" && c.what.id === pad.id)
                    .map((clip) => {
                      const p = placed(clip);
                      return (
                        <div
                          key={clip.id}
                          className={`clip ${picked.includes(clip.id) ? "picked" : ""}`}
                          style={{ left: p.start * pps, width: Math.max(6, p.length * pps), background: what.colour }}
                          title={`${what.name}, ${clockTime(p.start)} to ${clockTime(p.start + p.length)}`}
                          onPointerDown={(e) => grab(e, clip)}
                        >
                          <span
                            className="clip-end"
                            title="Drag to make it longer or shorter"
                            onPointerDown={(e) => {
                              setPicked([clip.id]);
                              begin(e, { kind: "stretch", id: clip.id, start: clip.start });
                            }}
                          />
                        </div>
                      );
                    })}
                  {ghost?.pad === pad.id && <div className="clip ghost" style={{ left: ghost.start * pps, width: Math.max(6, ghost.length * pps) }} />}
                </div>
              );
            })}
            {/* Past the end of the timeline there is nothing to place. */}
            <div className="lanes-end" style={{ left: width }} aria-hidden />
            <div className="playhead" ref={head} aria-hidden />
          </div>
        </div>
      </div>
      <p className="hint timeline-hint">
        {pads.length === 0
          ? "Pads get a lane each here. Add some in the Live view: a white flash, a strobe, a colour, any effect."
          : picked.length > 0
            ? `${picked.length} picked. Drag to move, drag the right-hand edge to stretch, Delete to remove. Shift+click picks several.`
            : "Drag in a pad's lane to hold that pad there. Click the Scenes lane to change scene. Or press Record and Play, and perform it."}
      </p>
    </section>
  );
}
