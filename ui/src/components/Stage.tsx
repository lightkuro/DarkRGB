// The stage: a live picture of the lights, where fixtures are arranged by
// dragging. The LEDs are drawn with WebGL; outlines and handles sit on top
// in SVG so they stay crisp at any zoom.
//
// Lights are picked here: click one, Shift+click to add or remove,
// Shift+drag (or the Select tool) to draw a box, double-click for a whole
// group. What is picked moves and turns as one, and the Lights list and
// the effects see the same selection.

import {
  ArrowLeftRight,
  BoxSelect,
  FolderPlus,
  Maximize,
  RotateCw,
  ShieldCheck,
  Sparkles,
  ZapOff,
} from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  engine,
  firstKind,
  perFrame,
  select,
  selection,
  settingsPage,
  takeFirstKind,
  useFirstKind,
  useFull,
  useSelection,
  useShow,
  useStatus,
  type Fixture,
} from "../engine";
import { StageRenderer, type View } from "../stageGl";
import { AddByAddress, AngleField, FoundDevices } from "./DevicePanel";
import { KindCards } from "./KindCards";

type Drag =
  | { kind: "pan"; startX: number; startY: number; view: View; moved: boolean }
  // A box being drawn. `base` is what was picked before, kept with Shift.
  | { kind: "box"; startX: number; startY: number; base: number[] }
  | { kind: "move"; startX: number; startY: number; fixtures: Fixture[] }
  | { kind: "resize"; fixture: Fixture }
  // Turning one light follows the pointer. Turning several turns them
  // about `centre`, by how far the pointer has gone round since `from`.
  | { kind: "rotate"; fixtures: Fixture[]; centre: [number, number]; from: number };

/** Half-height of a strip's outline, in screen pixels, so it is easy to grab. */
const STRIP_GRIP = 11;
const MARGIN = 6;

const sendFixtures = perFrame((fixtures: Fixture[]) => {
  for (const f of fixtures) {
    engine.quiet({
      type: "update_fixture",
      id: f.id,
      x: f.x,
      y: f.y,
      width: f.width,
      height: f.height,
      rotation: f.rotation,
    });
  }
});

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

/** The box around some fixtures, in canvas units: left, top, right, bottom. */
function bounds(fixtures: Fixture[]): [number, number, number, number] {
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const f of fixtures) {
    for (const [x, y] of corners(f)) {
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x);
      minY = Math.min(minY, y);
      maxY = Math.max(maxY, y);
    }
  }
  return [minX, minY, maxX, maxY];
}

function fitView(fixtures: Fixture[], width: number, height: number): View {
  if (fixtures.length === 0 || width === 0 || height === 0) return { cx: 0, cy: 0, scale: 300 };
  const [minX, minY, maxX, maxY] = bounds(fixtures);
  const pad = 56;
  const scale = Math.min(
    (width - pad * 2) / Math.max(maxX - minX, 0.05),
    (height - pad * 2) / Math.max(maxY - minY, 0.05),
  );
  return { cx: (minX + maxX) / 2, cy: (minY + maxY) / 2, scale: Math.min(Math.max(scale, 10), 6000) };
}

/** Turns fixtures about a point by `degrees`, as one rigid thing. */
function turned(fixtures: Fixture[], [cx, cy]: [number, number], degrees: number): Fixture[] {
  const rad = (degrees * Math.PI) / 180;
  const [cos, sin] = [Math.cos(rad), Math.sin(rad)];
  return fixtures.map((f) => {
    const [dx, dy] = [f.x - cx, f.y - cy];
    return {
      ...f,
      x: cx + dx * cos - dy * sin,
      y: cy + dx * sin + dy * cos,
      rotation: (((f.rotation + degrees) % 360) + 360) % 360,
    };
  });
}

/** Settles on a tidy angle when close to one. */
function tidy(angle: number): number {
  const near = Math.round(angle / 15) * 15;
  return Math.abs(angle - near) < 4 ? near : angle;
}

export function Stage() {
  const show = useShow();
  const status = useStatus();
  const picked = useSelection();
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [view, setViewState] = useState<View>({ cx: 0, cy: 0, scale: 300 });
  const viewRef = useRef(view);
  // Fixtures being dragged, as they are under the pointer right now. The
  // engine's copies follow a moment later.
  const [live, setLive] = useState<Fixture[]>([]);
  // The box being drawn to pick lights, in screen pixels inside the stage.
  const [box, setBox] = useState<{ x: number; y: number; w: number; h: number } | null>(null);
  // The Select tool: dragging on empty space draws a box instead of moving the view.
  const [boxTool, setBoxTool] = useState(false);
  const dragRef = useRef<Drag | null>(null);
  // Once the user pans or zooms, stop refitting the view for them.
  const userView = useRef(false);
  const [glError, setGlError] = useState<string | null>(null);

  const fixtures = show?.fixtures ?? [];
  // A show nobody has touched yet is asked who it is for. The answer is
  // applied when its first lights arrive, by whichever way they are added.
  const full = useFull();
  const kind = useFirstKind();
  const fresh = Boolean(show && show.kind === "custom" && show.scenes.length === 0 && show.layers.length <= 1);
  const hadLights = useRef(true);
  useEffect(() => {
    const has = fixtures.length > 0;
    if (has && !hadLights.current && fresh) {
      const mine = takeFirstKind();
      if (mine && mine !== "custom") engine.send({ type: "begin", kind: mine, demo: false });
    }
    hadLights.current = has;
  }, [fixtures.length, fresh]);
  const fixturesRef = useRef(fixtures);
  fixturesRef.current = live.length ? fixtures.map((f) => live.find((l) => l.id === f.id) ?? f) : fixtures;
  const shown = fixturesRef.current;
  const chosen = shown.filter((f) => picked.includes(f.id));

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

  // Once the engine has caught up with a finished drag, drop our copies.
  useEffect(() => {
    if (!dragRef.current) setLive([]);
  }, [show]);

  // Lights that no longer exist cannot stay picked.
  useEffect(() => {
    const ids = fixtureKey ? fixtureKey.split(",").map(Number) : [];
    const current = selection.get();
    if (current.some((id) => !ids.includes(id))) selection.set(current.filter((id) => ids.includes(id)));
  }, [fixtureKey]);

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
      const cx = v.cx + px / v.scale - px / scale;
      const cy = v.cy + py / v.scale - py / scale;
      userView.current = true;
      setView({ cx, cy, scale });
    };
    wrap.addEventListener("wheel", onWheel, { passive: false });
    return () => wrap.removeEventListener("wheel", onWheel);
  }, [setView]);

  const toScreen = (x: number, y: number): [number, number] => [
    (x - view.cx) * view.scale + size.width / 2,
    (y - view.cy) * view.scale + size.height / 2,
  ];

  /** Where the pointer is on the canvas, in canvas units. */
  const toCanvas = (e: { clientX: number; clientY: number }): [number, number] => {
    const rect = wrapRef.current!.getBoundingClientRect();
    const v = viewRef.current;
    return [
      (e.clientX - rect.left - rect.width / 2) / v.scale + v.cx,
      (e.clientY - rect.top - rect.height / 2) / v.scale + v.cy,
    ];
  };

  const begin = (e: React.PointerEvent, drag: Drag) => {
    e.stopPropagation();
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    dragRef.current = drag;
  };

  const moveTo = (next: Fixture[]) => {
    setLive(next);
    sendFixtures(next);
  };

  const onMove = (e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;
    const scale = viewRef.current.scale;
    switch (drag.kind) {
      case "pan": {
        const [dx, dy] = [e.clientX - drag.startX, e.clientY - drag.startY];
        if (Math.abs(dx) + Math.abs(dy) > 3) {
          drag.moved = true;
          userView.current = true;
        }
        setView({ ...drag.view, cx: drag.view.cx - dx / drag.view.scale, cy: drag.view.cy - dy / drag.view.scale });
        return;
      }
      case "box": {
        const rect = wrapRef.current!.getBoundingClientRect();
        const [x0, y0] = [drag.startX - rect.left, drag.startY - rect.top];
        const [x1, y1] = [e.clientX - rect.left, e.clientY - rect.top];
        const next = { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
        setBox(next);
        // A light is picked when its middle or any corner is inside the box.
        const inside = (px: number, py: number) => {
          const [sx, sy] = toScreen(px, py);
          return sx >= next.x && sx <= next.x + next.w && sy >= next.y && sy <= next.y + next.h;
        };
        const hit = fixturesRef.current
          .filter((f) => inside(f.x, f.y) || corners(f).some(([cx, cy]) => inside(cx, cy)))
          .map((f) => f.id);
        selection.set([...new Set([...drag.base, ...hit])]);
        return;
      }
      case "move": {
        const [dx, dy] = [(e.clientX - drag.startX) / scale, (e.clientY - drag.startY) / scale];
        moveTo(drag.fixtures.map((f) => ({ ...f, x: f.x + dx, y: f.y + dy })));
        return;
      }
      case "rotate": {
        const [wx, wy] = toCanvas(e);
        const angle = (Math.atan2(wy - drag.centre[1], wx - drag.centre[0]) * 180) / Math.PI;
        if (drag.fixtures.length === 1) {
          // One light: its handle follows the pointer, settling on tidy angles.
          const f = drag.fixtures[0]!;
          const rotation = tidy(angle + 90);
          moveTo([{ ...f, rotation: ((rotation % 360) + 360) % 360 }]);
        } else {
          moveTo(turned(drag.fixtures, drag.centre, tidy(angle - drag.from)));
        }
        return;
      }
      case "resize": {
        const f = drag.fixture;
        const [wx, wy] = toCanvas(e);
        const rad = (-f.rotation * Math.PI) / 180;
        const along = Math.abs((wx - f.x) * Math.cos(rad) - (wy - f.y) * Math.sin(rad));
        const width = Math.max(0.02, along * 2);
        // A matrix keeps its LEDs square.
        const height = f.shape.type === "matrix" ? (width * f.shape.rows) / f.shape.cols : 0;
        moveTo([{ ...f, width, height }]);
      }
    }
  };

  const onUp = () => {
    const drag = dragRef.current;
    dragRef.current = null;
    setBox(null);
    // A click on empty space, as opposed to a drag, lets go of the selection.
    if (drag?.kind === "pan" && !drag.moved) selection.set([]);
  };

  // Where picked lights have been nudged to. Key presses can arrive faster
  // than the engine reports each move back, so each one builds on the last
  // instead of on the engine's older position.
  const nudged = useRef<{ at: Map<number, [number, number]>; timer?: ReturnType<typeof setTimeout> }>({
    at: new Map(),
  });

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") return selection.set([]);
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      return selection.set(fixtures.map((f) => f.id));
    }
    const step = (e.shiftKey ? 20 : 4) / view.scale;
    const delta: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, -step],
      ArrowDown: [0, step],
    };
    const d = delta[e.key];
    if (!d || chosen.length === 0) return;
    e.preventDefault();
    clearTimeout(nudged.current.timer);
    for (const f of chosen) {
      const [x, y] = nudged.current.at.get(f.id) ?? [f.x, f.y];
      const next: [number, number] = [x + d[0], y + d[1]];
      nudged.current.at.set(f.id, next);
      engine.send({ type: "update_fixture", id: f.id, x: next[0], y: next[1] });
    }
    // Forget our own positions once the keys have been quiet for a moment.
    nudged.current.timer = setTimeout(() => nudged.current.at.clear(), 500);
  };

  // Virtual lights get a dashed outline, so they are not mistaken for real ones.
  const virtualIds = new Set(show?.devices.filter((d) => d.kind === "virtual").map((d) => d.id));
  const pixels = fixtures.reduce(
    (n, f) => n + (f.shape.type === "strip" ? f.shape.count : f.shape.cols * f.shape.rows),
    0,
  );
  const single = chosen.length === 1 ? chosen[0]! : null;
  // The box around several picked lights, with one handle to turn them all.
  const around = chosen.length > 1 ? bounds(chosen) : null;
  const centre: [number, number] | null = around ? [(around[0] + around[2]) / 2, (around[1] + around[3]) / 2] : null;

  const turnQuarter = () => {
    if (single) {
      engine.send({ type: "update_fixture", id: single.id, rotation: (Math.round(single.rotation / 90) * 90 + 90) % 360 });
    } else if (centre) {
      for (const f of turned(chosen, centre, 90)) {
        engine.send({ type: "update_fixture", id: f.id, x: f.x, y: f.y, rotation: f.rotation });
      }
    }
  };

  const groupPicked = () => {
    const taken = show?.light_groups.map((g) => g.name) ?? [];
    let n = taken.length + 1;
    while (taken.includes(`Group ${n}`)) n += 1;
    engine.send({ type: "add_light_group", name: `Group ${n}`, fixtures: picked });
  };

  const handlers = { onPointerMove: onMove, onPointerUp: onUp, onPointerCancel: onUp };

  return (
    <section
      className={`stage ${boxTool ? "boxing" : ""}`}
      ref={wrapRef}
      tabIndex={0}
      aria-label="Stage. Drag lights to arrange them. Shift and drag to pick several."
      onKeyDown={onKey}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        if (e.shiftKey || boxTool) {
          begin(e, { kind: "box", startX: e.clientX, startY: e.clientY, base: e.shiftKey ? selection.get() : [] });
          if (!e.shiftKey) selection.set([]);
        } else {
          begin(e, { kind: "pan", startX: e.clientX, startY: e.clientY, view: viewRef.current, moved: false });
        }
      }}
      {...handlers}
    >
      <canvas ref={canvasRef} className="stage-canvas" />

      <svg className="stage-overlay" width={size.width} height={size.height} aria-hidden>
        {shown.map((f) => {
          const [sx, sy] = toScreen(f.x, f.y);
          const w = f.width * view.scale + MARGIN * 2;
          const h = (f.shape.type === "strip" ? 0 : f.height * view.scale) + Math.max(MARGIN, STRIP_GRIP) * 2;
          const isPicked = picked.includes(f.id);
          return (
            <g key={f.id} transform={`translate(${sx} ${sy}) rotate(${f.rotation})`}>
              <rect
                className={`fixture-box ${isPicked ? "selected" : ""} ${virtualIds.has(f.device) ? "virtual" : ""}`}
                x={-w / 2}
                y={-h / 2}
                width={w}
                height={h}
                rx={7}
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  if (e.shiftKey) {
                    // Add to, or take from, what is picked. No drag.
                    e.stopPropagation();
                    return select([f.id], true);
                  }
                  // Dragging one of several picked lights moves them all.
                  const moving = isPicked ? chosen : [f];
                  if (!isPicked) select([f.id]);
                  begin(e, { kind: "move", startX: e.clientX, startY: e.clientY, fixtures: moving });
                }}
                onDoubleClick={() => {
                  // The whole group this light is filed under.
                  if (f.group !== null) select(shown.filter((x) => x.group === f.group).map((x) => x.id));
                }}
                {...handlers}
              />
              {single?.id === f.id && (
                <>
                  <line className="handle-stem" x1={0} y1={-h / 2} x2={0} y2={-h / 2 - 22} />
                  <circle
                    className="handle rotate"
                    cx={0}
                    cy={-h / 2 - 22}
                    r={7}
                    onPointerDown={(e) => begin(e, { kind: "rotate", fixtures: [f], centre: [f.x, f.y], from: 0 })}
                    {...handlers}
                  >
                    <title>Drag to turn</title>
                  </circle>
                  <rect
                    className="handle resize"
                    x={w / 2 - 6}
                    y={-9}
                    width={12}
                    height={18}
                    rx={4}
                    onPointerDown={(e) => begin(e, { kind: "resize", fixture: f })}
                    {...handlers}
                  >
                    <title>Drag to resize</title>
                  </rect>
                </>
              )}
            </g>
          );
        })}
        {around &&
          centre &&
          (() => {
            const [left, top] = toScreen(around[0], around[1]);
            const [right, bottom] = toScreen(around[2], around[3]);
            const pad = 16;
            // Keep the handle on the stage even when the lights run off it.
            const mid = Math.min(Math.max((left + right) / 2, 16), size.width - 16);
            const grip = Math.min(Math.max(top - pad - 22, 16), size.height - 16);
            return (
              <g>
                <rect
                  className="selection-box"
                  x={left - pad}
                  y={top - pad}
                  width={right - left + pad * 2}
                  height={bottom - top + pad * 2}
                  rx={10}
                />
                <line className="handle-stem" x1={mid} y1={top - pad} x2={mid} y2={grip} />
                <circle
                  className="handle rotate"
                  cx={mid}
                  cy={grip}
                  r={7}
                  onPointerDown={(e) => {
                    const [wx, wy] = toCanvas(e);
                    const from = (Math.atan2(wy - centre[1], wx - centre[0]) * 180) / Math.PI;
                    begin(e, { kind: "rotate", fixtures: chosen, centre, from });
                  }}
                  {...handlers}
                >
                  <title>Drag to turn these together</title>
                </circle>
              </g>
            );
          })()}
        {box && <rect className="marquee" x={box.x} y={box.y} width={box.w} height={box.h} />}
      </svg>

      {glError && <p className="callout error stage-message">{glError}</p>}

      {show && fixtures.length === 0 && (
        <div className="stage-empty" onPointerDown={(e) => e.stopPropagation()} onWheel={(e) => e.stopPropagation()}>
          {fresh && (
            <>
              <h1>What are you lighting?</h1>
              <KindCards value={kind} onChange={(k) => firstKind.set(k)} />
            </>
          )}
          <h1>{fresh ? "Then add your lights" : "Add your lights"}</h1>
          <p>
            Devices running WLED on your network appear here by themselves. Add one and it shows up on this stage,
            already reacting to whatever your computer is playing.
          </p>
          <FoundDevices />
          <AddByAddress />
          <div className="or">
            <span>or</span>
          </div>
          <button
            type="button"
            className="button wide demo"
            onClick={() => engine.send({ type: "begin", kind: fresh ? kind : show.kind, demo: true })}
          >
            <Sparkles size={15} /> Try it with demo lights
          </button>
          <p className="hint">
            Demo lights exist only on this screen: two strips and a matrix. Play some music and watch.
          </p>
        </div>
      )}

      {show?.master.blackout && fixtures.length > 0 && (
        <p className="stage-banner">Blackout is on. The lights are off.</p>
      )}

      {/* One row along the bottom. When it gets too narrow for both, the
          numbers move up a line; nothing covers anything else. */}
      <div className="stage-foot">
        <div className="stage-tools" onPointerDown={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
          {chosen.length > 0 && (
            <div className="tool-group">
              <span className="tool-label">{single ? single.name : `${chosen.length} lights`}</span>
              <button type="button" className="tool" title="Turn a quarter turn" onClick={turnQuarter}>
                <RotateCw size={15} /> Turn
              </button>
              {full && single && <AngleField fixture={single} />}
              {full && single?.shape.type === "strip" && (
                <button
                  type="button"
                  className={`tool ${single.shape.reverse ? "active" : ""}`}
                  title="Swap which end the strip starts from"
                  aria-pressed={single.shape.reverse}
                  onClick={() =>
                    engine.send({
                      type: "update_fixture",
                      id: single.id,
                      reverse: single.shape.type === "strip" ? !single.shape.reverse : false,
                    })
                  }
                >
                  <ArrowLeftRight size={15} /> Reverse
                </button>
              )}
              {full && (
                <button
                  type="button"
                  className="tool"
                  title="Make a group of what is picked. Effects can play on a group, and it dims as one."
                  onClick={groupPicked}
                >
                  <FolderPlus size={15} /> Group
                </button>
              )}
            </div>
          )}
          {full && fixtures.length > 1 && (
            <button
              type="button"
              className={`tool ${boxTool ? "active" : ""}`}
              aria-pressed={boxTool}
              title="Pick several lights by dragging a box around them. Shift and drag does the same."
              onClick={() => setBoxTool(!boxTool)}
            >
              <BoxSelect size={15} /> Select
            </button>
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
            {show && show.flash.limit === 0 ? (
              <button
                type="button"
                className="stage-flag off"
                title="Fast flashing is not being softened. Click for the setting."
                onPointerDown={(e) => e.stopPropagation()}
                onClick={() => settingsPage.set("lights")}
              >
                <ZapOff size={12} /> Flash limit off
              </button>
            ) : (
              status?.render.flash_limited && (
                <button
                  type="button"
                  className="stage-flag"
                  title="Something is flashing faster than the limit allows, so it is being softened. Click for the setting."
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={() => settingsPage.set("lights")}
                >
                  <ShieldCheck size={12} /> Flash limit is softening this
                </button>
              )
            )}
            {pixels.toLocaleString()} LEDs
            {status ? ` · ${Math.round(status.render.fps)} frames per second` : ""}
          </p>
        )}
      </div>
    </section>
  );
}
