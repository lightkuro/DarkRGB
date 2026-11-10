// The stage: a live picture of the lights, where fixtures are arranged by
// dragging. The LEDs are drawn with WebGL; outlines and handles sit on top
// in SVG so they stay crisp at any zoom.

import { ArrowLeftRight, Maximize, RotateCw } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { engine, perFrame, useShow, useStatus, type Fixture } from "../engine";
import { StageRenderer, type View } from "../stageGl";
import { AddByAddress, FoundDevices } from "./DevicePanel";

type Drag =
  | { kind: "pan"; startX: number; startY: number; view: View }
  | { kind: "move" | "resize" | "rotate"; startX: number; startY: number; fixture: Fixture };

/** Half-height of a strip's outline, in screen pixels, so it is easy to grab. */
const STRIP_GRIP = 11;
const MARGIN = 6;

const sendFixture = perFrame((f: Fixture) =>
  engine.quiet({ type: "update_fixture", id: f.id, x: f.x, y: f.y, width: f.width, height: f.height, rotation: f.rotation }),
);

function corners(f: Fixture): [number, number][] {
  const rad = (f.rotation * Math.PI) / 180;
  const [cos, sin] = [Math.cos(rad), Math.sin(rad)];
  const [hw, hh] = [f.width / 2, f.height / 2];
  return [
    [-hw, -hh],
    [hw, -hh],
    [hw, hh],
    [-hw, hh],
  ].map(([lx, ly]) => [f.x + lx! * cos - ly! * sin, f.y + lx! * sin + ly! * cos] as [number, number]);
}

function fitView(fixtures: Fixture[], width: number, height: number): View {
  if (fixtures.length === 0 || width === 0 || height === 0) return { cx: 0, cy: 0, scale: 300 };
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const f of fixtures) {
    for (const [x, y] of corners(f)) {
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x);
      minY = Math.min(minY, y);
      maxY = Math.max(maxY, y);
    }
  }
  const pad = 56;
  const scale = Math.min(
    (width - pad * 2) / Math.max(maxX - minX, 0.05),
    (height - pad * 2) / Math.max(maxY - minY, 0.05),
  );
  return { cx: (minX + maxX) / 2, cy: (minY + maxY) / 2, scale: Math.min(Math.max(scale, 10), 6000) };
}

export function Stage() {
  const show = useShow();
  const status = useStatus();
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [view, setViewState] = useState<View>({ cx: 0, cy: 0, scale: 300 });
  const viewRef = useRef(view);
  const [selected, setSelected] = useState<number | null>(null);
  // The fixture being dragged, as it is under the pointer right now. The
  // engine's copy follows a moment later.
  const [live, setLive] = useState<Fixture | null>(null);
  const dragRef = useRef<Drag | null>(null);
  // Once the user pans or zooms, stop refitting the view for them.
  const userView = useRef(false);
  const [glError, setGlError] = useState<string | null>(null);

  const fixtures = show?.fixtures ?? [];
  const fixturesRef = useRef(fixtures);
  fixturesRef.current = live ? fixtures.map((f) => (f.id === live.id ? live : f)) : fixtures;
  const shown = fixturesRef.current;

  const setView = useCallback((next: View) => {
    viewRef.current = next;
    setViewState(next);
  }, []);

  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const observer = new ResizeObserver(() => setSize({ width: wrap.clientWidth, height: wrap.clientHeight }));
    observer.observe(wrap);
    return () => observer.disconnect();
  }, []);

  // Refit when fixtures are added or removed, or the stage is resized.
  const fixtureKey = fixtures.map((f) => f.id).join(",");
  useEffect(() => {
    if (!userView.current) setView(fitView(fixturesRef.current, size.width, size.height));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fixtureKey, size.width, size.height]);

  // Once the engine has caught up with a finished drag, drop our copy.
  useEffect(() => {
    if (!dragRef.current) setLive(null);
  }, [show]);

  useEffect(() => {
    if (selected !== null && !fixtures.some((f) => f.id === selected)) setSelected(null);
  }, [fixtures, selected]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let renderer: StageRenderer;
    try {
      renderer = new StageRenderer(canvas);
    } catch (e) {
      setGlError(e instanceof Error ? e.message : String(e));
      return;
    }
    let frame = 0;
    const draw = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const [w, h] = [Math.round(canvas.clientWidth * dpr), Math.round(canvas.clientHeight * dpr)];
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      if (w > 0 && h > 0) renderer.draw(viewRef.current, fixturesRef.current, dpr);
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  }, []);

  // Zoom around the pointer. Registered by hand because the page must not
  // scroll, which React's own wheel handler cannot prevent.
  useEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = wrap.getBoundingClientRect();
      const v = viewRef.current;
      const [px, py] = [e.clientX - rect.left - rect.width / 2, e.clientY - rect.top - rect.height / 2];
      const scale = Math.min(Math.max(v.scale * Math.exp(-e.deltaY * 0.0015), 10), 6000);
      // Keep the point under the pointer where it is.
      userView.current = true;
      setView({
        scale,
        cx: v.cx + px / v.scale - px / scale,
        cy: v.cy + py / v.scale - py / scale,
      });
    };
    wrap.addEventListener("wheel", onWheel, { passive: false });
    return () => wrap.removeEventListener("wheel", onWheel);
  }, [setView]);

  const toScreen = (x: number, y: number): [number, number] => [
    (x - view.cx) * view.scale + size.width / 2,
    (y - view.cy) * view.scale + size.height / 2,
  ];

  const begin = (e: React.PointerEvent, drag: Drag) => {
    e.stopPropagation();
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    dragRef.current = drag;
  };

  const onMove = (e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;
    const [dx, dy] = [e.clientX - drag.startX, e.clientY - drag.startY];
    if (drag.kind === "pan") {
      if (Math.abs(dx) + Math.abs(dy) > 3) userView.current = true;
      setView({ ...drag.view, cx: drag.view.cx - dx / drag.view.scale, cy: drag.view.cy - dy / drag.view.scale });
      return;
    }
    const f = drag.fixture;
    const scale = viewRef.current.scale;
    let next: Fixture;
    if (drag.kind === "move") {
      next = { ...f, x: f.x + dx / scale, y: f.y + dy / scale };
    } else {
      // Pointer position relative to the fixture's centre, in canvas units.
      const rect = wrapRef.current!.getBoundingClientRect();
      const wx = (e.clientX - rect.left - rect.width / 2) / scale + viewRef.current.cx - f.x;
      const wy = (e.clientY - rect.top - rect.height / 2) / scale + viewRef.current.cy - f.y;
      if (drag.kind === "rotate") {
        let angle = (Math.atan2(wy, wx) * 180) / Math.PI + 90;
        // Settle on tidy angles when close to one.
        const tidy = Math.round(angle / 15) * 15;
        if (Math.abs(angle - tidy) < 4) angle = tidy;
        next = { ...f, rotation: ((angle % 360) + 360) % 360 };
      } else {
        const rad = (-f.rotation * Math.PI) / 180;
        const along = Math.abs(wx * Math.cos(rad) - wy * Math.sin(rad));
        const width = Math.max(0.02, along * 2);
        // A matrix keeps its LEDs square.
        const height = f.shape.type === "matrix" ? (width * f.shape.rows) / f.shape.cols : 0;
        next = { ...f, width, height };
      }
    }
    setLive(next);
    sendFixture(next);
  };

  const onUp = () => {
    dragRef.current = null;
  };

  // Where the selected fixture has been nudged to. Key presses can arrive
  // faster than the engine reports each move back, so each one builds on
  // the last instead of on the engine's older position.
  const nudged = useRef<{ id: number; x: number; y: number; timer?: ReturnType<typeof setTimeout> } | null>(null);

  const nudge = (e: React.KeyboardEvent) => {
    const f = fixtures.find((x) => x.id === selected);
    if (!f) return;
    const step = (e.shiftKey ? 20 : 4) / view.scale;
    const delta: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, -step],
      ArrowDown: [0, step],
    };
    const d = delta[e.key];
    if (e.key === "Escape") setSelected(null);
    if (!d) return;
    e.preventDefault();
    const from = nudged.current?.id === f.id ? nudged.current : { id: f.id, x: f.x, y: f.y };
    clearTimeout(nudged.current?.timer);
    const next = { id: f.id, x: from.x + d[0], y: from.y + d[1] };
    // Forget our own position once the keys have been quiet for a moment.
    nudged.current = { ...next, timer: setTimeout(() => (nudged.current = null), 500) };
    engine.send({ type: "update_fixture", ...next });
  };

  const selectedFixture = shown.find((f) => f.id === selected);
  const pixels = fixtures.reduce(
    (n, f) => n + (f.shape.type === "strip" ? f.shape.count : f.shape.cols * f.shape.rows),
    0,
  );

  return (
    <section
      className="stage"
      ref={wrapRef}
      tabIndex={0}
      aria-label="Stage. Drag lights to arrange them."
      onKeyDown={nudge}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        setSelected(null);
        begin(e, { kind: "pan", startX: e.clientX, startY: e.clientY, view: viewRef.current });
      }}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={onUp}
    >
      <canvas ref={canvasRef} className="stage-canvas" />

      <svg className="stage-overlay" width={size.width} height={size.height} aria-hidden>
        {shown.map((f) => {
          const [sx, sy] = toScreen(f.x, f.y);
          const w = f.width * view.scale + MARGIN * 2;
          const h = (f.shape.type === "strip" ? 0 : f.height * view.scale) + Math.max(MARGIN, STRIP_GRIP) * 2;
          const isSelected = f.id === selected;
          return (
            <g key={f.id} transform={`translate(${sx} ${sy}) rotate(${f.rotation})`}>
              <rect
                className={`fixture-box ${isSelected ? "selected" : ""}`}
                x={-w / 2}
                y={-h / 2}
                width={w}
                height={h}
                rx={7}
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  setSelected(f.id);
                  begin(e, { kind: "move", startX: e.clientX, startY: e.clientY, fixture: f });
                }}
                onPointerMove={onMove}
                onPointerUp={onUp}
                onPointerCancel={onUp}
              />
              {isSelected && (
                <>
                  <line className="handle-stem" x1={0} y1={-h / 2} x2={0} y2={-h / 2 - 22} />
                  <circle
                    className="handle rotate"
                    cx={0}
                    cy={-h / 2 - 22}
                    r={7}
                    onPointerDown={(e) => begin(e, { kind: "rotate", startX: e.clientX, startY: e.clientY, fixture: f })}
                    onPointerMove={onMove}
                    onPointerUp={onUp}
                    onPointerCancel={onUp}
                  >
                    <title>Drag to rotate</title>
                  </circle>
                  <rect
                    className="handle resize"
                    x={w / 2 - 6}
                    y={-9}
                    width={12}
                    height={18}
                    rx={4}
                    onPointerDown={(e) => begin(e, { kind: "resize", startX: e.clientX, startY: e.clientY, fixture: f })}
                    onPointerMove={onMove}
                    onPointerUp={onUp}
                    onPointerCancel={onUp}
                  >
                    <title>Drag to resize</title>
                  </rect>
                </>
              )}
            </g>
          );
        })}
      </svg>

      {glError && <p className="callout error stage-message">{glError}</p>}

      {show && fixtures.length === 0 && (
        <div className="stage-empty" onPointerDown={(e) => e.stopPropagation()}>
          <h1>Add your lights</h1>
          <p>
            Devices running WLED on your network appear here by themselves. Add one and it shows up on this stage,
            already reacting to whatever your computer is playing.
          </p>
          <FoundDevices />
          <AddByAddress autoFocus />
          <p className="hint">
            No hardware to hand? Run the bundled simulator (<code>ols-sim</code>) and add{" "}
            <code>127.0.0.1:8081</code>.
          </p>
        </div>
      )}

      {show?.master.blackout && fixtures.length > 0 && (
        <p className="stage-banner">Blackout is on. The lights are off.</p>
      )}

      <div className="stage-tools" onPointerDown={(e) => e.stopPropagation()}>
        {selectedFixture && (
          <div className="tool-group">
            <span className="tool-label">{selectedFixture.name}</span>
            <button
              type="button"
              className="tool"
              title="Rotate a quarter turn"
              onClick={() =>
                engine.send({
                  type: "update_fixture",
                  id: selectedFixture.id,
                  rotation: (Math.round(selectedFixture.rotation / 90) * 90 + 90) % 360,
                })
              }
            >
              <RotateCw size={15} /> Rotate
            </button>
            {selectedFixture.shape.type === "strip" && (
              <button
                type="button"
                className={`tool ${selectedFixture.shape.reverse ? "active" : ""}`}
                title="Swap which end the strip starts from"
                aria-pressed={selectedFixture.shape.reverse}
                onClick={() =>
                  engine.send({
                    type: "update_fixture",
                    id: selectedFixture.id,
                    reverse: selectedFixture.shape.type === "strip" ? !selectedFixture.shape.reverse : false,
                  })
                }
              >
                <ArrowLeftRight size={15} /> Reverse
              </button>
            )}
          </div>
        )}
        {fixtures.length > 0 && (
          <button
            type="button"
            className="tool"
            title="Show all lights"
            onClick={() => {
              userView.current = false;
              setView(fitView(shown, size.width, size.height));
            }}
          >
            <Maximize size={15} /> Fit
          </button>
        )}
      </div>

      {fixtures.length > 0 && (
        <p className="stage-stats">
          {pixels.toLocaleString()} LEDs
          {status ? ` · ${Math.round(status.render.fps)} frames per second` : ""}
        </p>
      )}
    </section>
  );
}
