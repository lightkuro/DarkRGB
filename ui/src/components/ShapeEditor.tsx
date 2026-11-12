// Bending a strip into a shape. The strip passes through corners inside
// its box on the stage, and its LEDs are spread evenly along the way. A
// shape is picked ready-made or drawn: click to add a corner, drag one to
// move it, double-click one to take it out.

import { Shapes, X } from "lucide-react";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { engine, perFrame, Store, useShow, type Fixture } from "../engine";
import { Switch } from "./ui";

type Point = [number, number];

/** The strip whose shape is being edited, if any. */
export const shaping = new Store<number | null>(null);

const around = (n: number, at: (t: number) => Point): Point[] => Array.from({ length: n }, (_, i) => at(i / n));
const circle = (n: number, r = 0.5, turn = -0.25): Point[] =>
  around(n, (t) => [0.5 + r * Math.cos((t + turn) * 2 * Math.PI), 0.5 + r * Math.sin((t + turn) * 2 * Math.PI)]);

/** Ready-made shapes. Corners run from (0, 0) at the top left to (1, 1). */
export const PRESETS: { name: string; points: Point[]; closed: boolean }[] = [
  { name: "Straight", points: [], closed: false },
  { name: "Corner", points: [[0, 0], [0, 1], [1, 1]], closed: false },
  { name: "Arch", points: Array.from({ length: 17 }, (_, i): Point => [0.5 - 0.5 * Math.cos((i / 16) * Math.PI), 1 - Math.sin((i / 16) * Math.PI)]), closed: false },
  { name: "Circle", points: circle(32), closed: true },
  { name: "Square", points: [[0, 0], [1, 0], [1, 1], [0, 1]], closed: true },
  { name: "Triangle", points: [[0.5, 0], [1, 1], [0, 1]], closed: true },
  { name: "Star", points: around(10, (t) => { const r = Math.round(t * 10) % 2 === 0 ? 0.5 : 0.2; const a = (t - 0.25) * 2 * Math.PI; return [0.5 + r * Math.cos(a), 0.5 + r * Math.sin(a)]; }), closed: true },
  { name: "Zigzag", points: Array.from({ length: 8 }, (_, i): Point => [i / 7, i % 2 === 0 ? 1 : 0]), closed: false },
  { name: "Up and down", points: [[0, 1], [0, 0], [0.33, 0], [0.33, 1], [0.67, 1], [0.67, 0], [1, 0], [1, 1]], closed: false },
  { name: "Spiral", points: Array.from({ length: 49 }, (_, i): Point => { const t = i / 48; const a = t * 6 * Math.PI; return [0.5 + 0.5 * t * Math.cos(a), 0.5 + 0.5 * t * Math.sin(a)]; }), closed: false },
];

const MAX_POINTS = 64;

/** The length of the way a strip takes across the stage. */
export function runLength(f: Fixture): number {
  const path = f.path ?? [];
  if (f.shape.type !== "strip" || path.length < 2) return f.width;
  const corners = f.closed ? [...path, path[0]!] : path;
  let total = 0;
  for (let i = 1; i < corners.length; i++) {
    total += Math.hypot((corners[i]![0] - corners[i - 1]![0]) * f.width, (corners[i]![1] - corners[i - 1]![1]) * f.height);
  }
  return total;
}

/** `count` places evenly along the corners, as the engine spreads LEDs. */
function along(points: Point[], closed: boolean, count: number, w: number, h: number): Point[] {
  if (points.length < 2) {
    return Array.from({ length: count }, (_, i): Point => [((i + 0.5) / count) * w, h / 2]);
  }
  const corners = (closed ? [...points, points[0]!] : points).map(([x, y]): Point => [x * w, y * h]);
  const legs = corners.slice(1).map((b, i) => Math.hypot(b[0] - corners[i]![0], b[1] - corners[i]![1]));
  const total = legs.reduce((a, b) => a + b, 0);
  const out: Point[] = [];
  let leg = 0;
  let before = 0;
  for (let i = 0; i < count; i++) {
    const at = ((i + 0.5) / count) * total;
    while (leg < legs.length - 1 && at > before + legs[leg]!) before += legs[leg++]!;
    const t = legs[leg]! > 0 ? Math.min(1, Math.max(0, (at - before) / legs[leg]!)) : 0;
    const [a, b] = [corners[leg]!, corners[leg + 1]!];
    out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
  }
  return out;
}

const sendPath = perFrame((value: { fixture: number; points: Point[]; closed: boolean }) =>
  engine.send({ type: "set_path", ...value }),
);

export function ShapeEditor() {
  const id = useSyncExternalStore(shaping.subscribe, shaping.get);
  const show = useShow();
  const fixture = show?.fixtures.find((f) => f.id === id && f.shape.type === "strip");
  const [points, setPoints] = useState<Point[]>([]);
  const [closed, setClosed] = useState(false);
  const [held, setHeld] = useState<number | null>(null);
  const svg = useRef<SVGSVGElement>(null);
  // The corner last pressed and when, to tell a double press.
  const pressed = useRef({ corner: -1, at: 0 });

  // Start from the strip's shape as it is when the window opens.
  useEffect(() => {
    if (!fixture) return;
    setPoints((fixture.path ?? []).map(([x, y]): Point => [x, y]));
    setClosed(fixture.closed ?? false);
    // Only when a different strip is opened, not on every echo of our own changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);
  useEffect(() => {
    if (id === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") shaping.set(null);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [id]);

  if (id === null || !fixture || fixture.shape.type !== "strip") return null;
  const count = fixture.shape.count;

  const change = (next: Point[], loop = closed) => {
    setPoints(next);
    setClosed(loop);
    // One corner is not a shape yet; wait for the second.
    if (next.length !== 1) sendPath({ fixture: fixture.id, points: next, closed: loop && next.length >= 3 });
  };

  // The box is drawn in the proportions it has on the stage.
  const ratio = fixture.height > 0 && fixture.width > 0 ? Math.min(2, Math.max(0.25, fixture.height / fixture.width)) : 1;
  const W = ratio > 1 ? 300 / ratio : 300;
  const H = ratio > 1 ? 300 : 300 * ratio;
  const PAD = 18;
  const place = (e: { clientX: number; clientY: number }): Point => {
    const box = svg.current!.getBoundingClientRect();
    const clamp = (v: number) => Math.min(1, Math.max(0, v));
    return [clamp((e.clientX - box.left - PAD) / W), clamp((e.clientY - box.top - PAD) / H)];
  };
  const add = (e: React.PointerEvent) => {
    if (points.length >= MAX_POINTS) return;
    const p = place(e);
    // Next to a stretch of the line, the corner goes into that stretch.
    // Anywhere else, it is added at the end.
    let best = { at: points.length, distance: 14 };
    const corners = closed && points.length >= 3 ? [...points, points[0]!] : points;
    for (let i = 1; i < corners.length; i++) {
      const [a, b] = [corners[i - 1]!, corners[i]!];
      const [ax, ay, bx, by, px, py] = [a[0] * W, a[1] * H, b[0] * W, b[1] * H, p[0] * W, p[1] * H];
      const length = (bx - ax) ** 2 + (by - ay) ** 2;
      const t = length > 0 ? Math.min(1, Math.max(0, ((px - ax) * (bx - ax) + (py - ay) * (by - ay)) / length)) : 0;
      const distance = Math.hypot(px - (ax + (bx - ax) * t), py - (ay + (by - ay) * t));
      if (distance < best.distance) best = { at: i, distance };
    }
    const next = [...points];
    next.splice(best.at, 0, p);
    change(next);
    setHeld(best.at);
  };
  const dots = along(points, closed && points.length >= 3, Math.min(count, 300), W, H);
  const line = points.map(([x, y]) => `${x * W},${y * H}`).join(" ");

  return (
    <div
      className="dialog-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) shaping.set(null);
      }}
    >
      <div className="dialog shape-editor" role="dialog" aria-modal="true" aria-label={`Shape of ${fixture.name}`}>
        <div className="settings-head">
          <h2>
            <Shapes size={16} aria-hidden /> Shape of {fixture.name}
          </h2>
          <button type="button" className="icon-button" aria-label="Close" onClick={() => shaping.set(null)}>
            <X size={16} />
          </button>
        </div>
        <div className="shape-body">
          <div className="shape-presets" role="group" aria-label="Ready-made shapes">
            {PRESETS.map((preset) => (
              <button key={preset.name} type="button" className="chip" onClick={() => change(preset.points, preset.closed)}>
                {preset.name}
              </button>
            ))}
          </div>
          <svg
            ref={svg}
            className="shape-canvas"
            width={W + PAD * 2}
            height={H + PAD * 2}
            role="img"
            aria-label="The strip's shape. Click to add a corner."
            onPointerDown={(e) => {
              if (e.button === 0 && e.target === e.currentTarget) add(e);
            }}
            onPointerMove={(e) => {
              if (held === null || e.buttons === 0) return;
              change(points.map((p, i) => (i === held ? place(e) : p)));
            }}
            onPointerUp={() => setHeld(null)}
          >
            <g transform={`translate(${PAD} ${PAD})`} pointerEvents="none">
              <rect className="shape-box" width={W} height={H} />
              {points.length >= 2 &&
                (closed && points.length >= 3 ? (
                  <polygon className="shape-line" points={line} />
                ) : (
                  <polyline className="shape-line" points={line} />
                ))}
              {dots.map(([x, y], i) => (
                <circle key={i} className={`shape-led ${i === 0 ? "first" : ""}`} cx={x} cy={y} r={i === 0 ? 3.5 : 2} />
              ))}
            </g>
            <g transform={`translate(${PAD} ${PAD})`}>
              {points.map(([x, y], i) => (
                <circle
                  key={i}
                  className={`shape-corner ${held === i ? "held" : ""}`}
                  cx={x * W}
                  cy={y * H}
                  r={7}
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    // A second press on the same corner straight after the
                    // first takes it out. (Holding the pointer for a drag
                    // sends the browser's own double-click elsewhere.)
                    const now = performance.now();
                    const again = pressed.current.corner === i && now - pressed.current.at < 400;
                    pressed.current = { corner: i, at: now };
                    if (again) {
                      pressed.current = { corner: -1, at: 0 };
                      // Two corners is the least a shape can have.
                      if (points.length > 2) change(points.filter((_, n) => n !== i));
                      return;
                    }
                    (e.currentTarget.ownerSVGElement as SVGSVGElement).setPointerCapture(e.pointerId);
                    setHeld(i);
                  }}
                >
                  <title>Corner {i + 1}. Drag to move, double-click to remove.</title>
                </circle>
              ))}
            </g>
          </svg>
          <p className="hint">
            {points.length < 2
              ? "A straight strip. Pick a shape above, or click twice in the box to start drawing one."
              : `${points.length} corners, ${count} LEDs spread evenly along the line. The larger dot is the first LED. Click to add a corner, drag to move one, double-click to take one out.`}
          </p>
          <div className="field inline">
            <span>
              Closed loop
              <span className="hint"> The strip runs on from its last corner back to its first.</span>
            </span>
            <Switch checked={closed && points.length >= 3} label="Closed loop" disabled={points.length < 3} onChange={(on) => change(points, on)} />
          </div>
          <div className="field inline">
            <span>Starts from the other end</span>
            <Switch
              checked={fixture.shape.reverse}
              label="Starts from the other end"
              onChange={(reverse) => engine.send({ type: "update_fixture", id: fixture.id, reverse })}
            />
          </div>
          <p className="hint">
            On the stage, the strip's box can be resized and turned like a matrix. An effect set to "Each light on
            its own" runs along the strip; otherwise it is drawn across the shape.
          </p>
          <div className="form-actions">
            <button type="button" className="button small" onClick={() => shaping.set(null)}>
              Done
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
