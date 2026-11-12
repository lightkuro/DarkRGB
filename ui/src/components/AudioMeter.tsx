// The audio readout in the top bar: a live spectrum, tempo, and a menu to
// pick the source. It is the first place to look when lights do not react.

import { ChevronDown, TriangleAlert, Volume2, VolumeX } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { engine, perFrame, useLive, useShow, useStatus } from "../engine";
import { Slider, useDismiss } from "./ui";

const setSensitivity = perFrame((sensitivity: number) => engine.quiet({ type: "set_audio", sensitivity }));

/** Where the sound comes from and how strongly the lights respond. */
export function AudioControls() {
  const status = useStatus();
  const show = useShow();
  const audio = status?.audio;
  const source = show?.audio.source ?? "system";
  return (
    <>
      {audio?.state === "error" && audio.message && <p className="callout error">{audio.message}</p>}
      <label className="field">
        <span className="field-head">
          <span>Listen to</span>
        </span>
        <select value={source} onChange={(e) => engine.send({ type: "set_audio", source: e.target.value })}>
          <option value="system">System audio (whatever this computer plays)</option>
          {status?.inputs.map((input) => (
            <option key={input.id} value={input.id}>
              {input.name}
            </option>
          ))}
          {source !== "system" && !status?.inputs.some((i) => i.id === source) && (
            <option value={source}>Unavailable input</option>
          )}
        </select>
      </label>
      <Slider
        label="Sensitivity"
        min={0.25}
        max={4}
        step={0.05}
        value={show?.audio.sensitivity ?? 1}
        format={(v) => `${v.toFixed(2)}×`}
        onChange={setSensitivity}
      />
      <p className="hint">
        Volume is levelled automatically, so quiet and loud music look the same. Raise sensitivity for a busier
        picture.
      </p>
      {audio?.state === "running" && (
        <p className="hint">
          {audio.sample_rate / 1000} kHz, {audio.channels} channel{audio.channels === 1 ? "" : "s"}
        </p>
      )}
    </>
  );
}

/** The tempo: found in the music, or tapped by hand. */
export function Tempo() {
  const live = useLive();
  const tapped = live.tapped_bpm > 0;
  return (
    <div className="field">
      <span className="field-head">
        <span>Tempo</span>
        <output>{tapped ? `${Math.round(live.tapped_bpm)} BPM, tapped` : "Following the music"}</output>
      </span>
      <div className="tempo-buttons">
        <button type="button" className="button small" onClick={() => engine.quiet({ type: "tap" })}>
          Tap
        </button>
        <button
          type="button"
          className="button small quiet"
          disabled={!tapped}
          onClick={() => engine.send({ type: "auto_tempo" })}
        >
          Follow the music
        </button>
      </div>
    </div>
  );
}

export function AudioMeter() {
  const status = useStatus();
  const show = useShow();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const tempoRef = useRef<HTMLSpanElement>(null);
  const beatRef = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const menuRef = useDismiss(open, close);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const [w, h] = [96, 22];
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    ctx.scale(dpr, dpr);
    let frame = 0;
    let lastTempo = "";
    const draw = () => {
      const a = engine.audio;
      ctx.clearRect(0, 0, w, h);
      const n = a.bands.length;
      const bar = w / n;
      for (let i = 0; i < n; i++) {
        const v = a.bands[i] ?? 0;
        const height = Math.max(1, v * h);
        // Amber for bass through teal for treble.
        ctx.fillStyle = a.active ? `hsl(${38 + (i / n) * 135} 90% ${48 + v * 14}%)` : "#2a2f3a";
        ctx.fillRect(i * bar, h - height, Math.max(1, bar - 1), height);
      }
      // A thin line along the top: how big this part of the song is.
      if (a.active) {
        ctx.fillStyle = a.drop > 0.3 ? "#ffffff" : "rgba(255, 255, 255, 0.45)";
        ctx.fillRect(0, 0, w * a.intensity, 1.5);
      }
      const tempo = a.bpm > 0 ? `${Math.round(a.bpm)} BPM` : a.active ? "no steady beat" : "silent";
      if (tempo !== lastTempo && tempoRef.current) {
        tempoRef.current.textContent = tempo;
        lastTempo = tempo;
      }
      if (beatRef.current) beatRef.current.style.opacity = String(0.15 + a.beat * 0.85);
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  }, []);

  const audio = status?.audio;
  const failed = audio?.state === "error";
  const source = show?.audio.source ?? "system";
  const listeningTo =
    audio?.state === "running"
      ? source === "system"
        ? `System audio · ${audio.device ?? ""}`
        : (audio.device ?? "Input")
      : failed
        ? "Audio problem"
        : "Starting audio…";

  return (
    <div className="audio" ref={menuRef}>
      <button
        type="button"
        className={`audio-button ${failed ? "failed" : ""}`}
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        title="Audio source and sensitivity"
      >
        {failed ? <TriangleAlert size={16} /> : audio?.state === "running" ? <Volume2 size={16} /> : <VolumeX size={16} />}
        <canvas ref={canvasRef} className="audio-bars" style={{ width: 96, height: 22 }} aria-hidden />
        <span className="audio-text">
          <span className="audio-tempo">
            <span className="beat-dot" ref={beatRef} />
            <span ref={tempoRef}>silent</span>
          </span>
          <span className="audio-device">{listeningTo}</span>
        </span>
        <ChevronDown size={14} />
      </button>

      {open && (
        <div className="popover audio-menu">
          <AudioControls />
          <Tempo />
        </div>
      )}
    </div>
  );
}
