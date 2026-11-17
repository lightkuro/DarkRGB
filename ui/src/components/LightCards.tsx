// The lights one by one, as LedFx shows its devices: a card for each,
// with what it shows right now and the effects that play on it. An
// effect can be given to one light from here, and any effect opened.

import { Eye, EyeOff, Grid3x3, Minus, Plus, SlidersHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  engine,
  layerHits,
  openEffect,
  stageModePref,
  useHello,
  useShow,
  type Fixture,
  type Layer,
  type Show,
} from "../engine";
import { EffectPicker } from "./EffectTiles";
import { swatch } from "./LayerPanel";
import { openMenu, SEPARATOR } from "./ContextMenu";
import { useDismiss } from "./ui";

/** Where each light's colours start in the preview: lights come in list order. */
function offsets(show: Show): Map<number, number> {
  const out = new Map<number, number>();
  let at = 0;
  for (const f of show.fixtures) {
    out.set(f.id, at);
    at += f.shape.type === "matrix" ? f.shape.cols * f.shape.rows : f.shape.count;
  }
  return out;
}

/**
 * Draws every card's light from the preview, as it arrives: a strip as a
 * line of its LEDs, a matrix by where each LED sits in it.
 */
function useLivePictures(show: Show | null, canvases: React.RefObject<Map<number, HTMLCanvasElement>>) {
  useEffect(() => {
    if (!show) return;
    const starts = offsets(show);
    let frame = 0;
    const draw = () => {
      frame = requestAnimationFrame(draw);
      const colors = engine.colors;
      const positions = engine.positions;
      if (engine.colorsSerial !== engine.layoutSerial) return;
      for (const f of show.fixtures) {
        const canvas = canvases.current?.get(f.id);
        const start = starts.get(f.id) ?? 0;
        const ctx = canvas?.getContext("2d");
        if (!canvas || !ctx) continue;
        if (f.shape.type === "matrix") {
          const { cols, rows } = f.shape;
          if (canvas.width !== cols || canvas.height !== rows) [canvas.width, canvas.height] = [cols, rows];
          const image = ctx.createImageData(cols, rows);
          // Each LED's place inside the light's box, turned back to upright.
          const angle = (-f.rotation * Math.PI) / 180;
          const [sin, cos] = [Math.sin(angle), Math.cos(angle)];
          for (let i = 0; i < cols * rows; i++) {
            const [px, py] = [positions[(start + i) * 2]! - f.x, positions[(start + i) * 2 + 1]! - f.y];
            const u = (px * cos - py * sin) / Math.max(f.width, 1e-6) + 0.5;
            const v = (px * sin + py * cos) / Math.max(f.height, 1e-6) + 0.5;
            const [c, r] = [Math.min(cols - 1, Math.max(0, Math.floor(u * cols))), Math.min(rows - 1, Math.max(0, Math.floor(v * rows)))];
            const o = (r * cols + c) * 4;
            image.data[o] = colors[(start + i) * 3] ?? 0;
            image.data[o + 1] = colors[(start + i) * 3 + 1] ?? 0;
            image.data[o + 2] = colors[(start + i) * 3 + 2] ?? 0;
            image.data[o + 3] = 255;
          }
          ctx.putImageData(image, 0, 0);
        } else {
          const count = f.shape.count;
          if (canvas.width !== count || canvas.height !== 1) [canvas.width, canvas.height] = [count, 1];
          const image = ctx.createImageData(count, 1);
          for (let i = 0; i < count; i++) {
            image.data.set([colors[(start + i) * 3] ?? 0, colors[(start + i) * 3 + 1] ?? 0, colors[(start + i) * 3 + 2] ?? 0, 255], i * 4);
          }
          ctx.putImageData(image, 0, 0);
        }
      }
    };
    draw();
    return () => cancelAnimationFrame(frame);
  }, [show, canvases]);
}

function EffectRow({ layer, fixture, show }: { layer: Layer; fixture: Fixture; show: Show }) {
  const hello = useHello();
  if (!hello) return null;
  const effect = hello.effects.find((e) => e.id === layer.effect);
  const own = layer.targets.length === 1 && layer.targets[0] === fixture.id;
  const where = layer.targets.length === 0 ? "on every light" : own ? "on this light only" : "on this and others";
  return (
    <li
      className={`light-effect${layer.enabled ? "" : " off"}`}
      onContextMenu={(e) =>
        openMenu(
          e,
          [
            { label: "Open its settings", icon: <SlidersHorizontal size={13} />, action: () => openEffect.set(layer.id) },
            {
              label: layer.enabled ? "Switch it off" : "Switch it on",
              icon: layer.enabled ? <EyeOff size={13} /> : <Eye size={13} />,
              action: () => engine.send({ type: "update_layer", id: layer.id, enabled: !layer.enabled }),
            },
            SEPARATOR,
            {
              label: own ? "Remove it" : "Leave this light out of it",
              icon: <Minus size={13} />,
              danger: own,
              action: () =>
                own
                  ? engine.send({ type: "remove_layer", id: layer.id })
                  : engine.send({
                      type: "update_layer",
                      id: layer.id,
                      targets: (layer.targets.length === 0 ? show.fixtures.map((f) => f.id) : layer.targets).filter((t) => t !== fixture.id),
                    }),
            },
          ],
          layer.name ?? effect?.name,
        )
      }
    >
      <span className="layer-swatch" style={{ background: swatch(layer, effect, hello) }} />
      <button type="button" className="light-effect-name" title="Open its settings in the effects panel" onClick={() => openEffect.set(layer.id)}>
        {layer.name ?? effect?.name ?? layer.effect}
        <span className="hint"> {where}</span>
      </button>
      <button
        type="button"
        className="icon-button"
        aria-label={layer.enabled ? `Switch ${effect?.name} off` : `Switch ${effect?.name} on`}
        aria-pressed={layer.enabled}
        onClick={() => engine.send({ type: "update_layer", id: layer.id, enabled: !layer.enabled })}
      >
        {layer.enabled ? <Eye size={14} /> : <EyeOff size={14} />}
      </button>
    </li>
  );
}

function LightCard({ fixture, show, canvas }: { fixture: Fixture; show: Show; canvas: (el: HTMLCanvasElement | null) => void }) {
  const [picking, setPicking] = useState(false);
  const ref = useDismiss(picking, () => setPicking(false));
  const layers = show.layers.filter((l) => layerHits(l, fixture));
  const group = show.light_groups.find((g) => g.id === fixture.group);
  const own = layers.find((l) => l.targets.length === 1 && l.targets[0] === fixture.id);
  const size = fixture.shape.type === "matrix" ? `${fixture.shape.cols} × ${fixture.shape.rows} matrix` : `${fixture.shape.count} LEDs`;
  const give = (effect: string) => {
    setPicking(false);
    // A light with an effect of its own has it changed; otherwise it is
    // given one, drawn over whatever else plays on it.
    if (own) engine.send({ type: "update_layer", id: own.id, effect });
    else engine.send({ type: "add_layer", effect, targets: [fixture.id] });
  };
  return (
    <article
      className="light-card"
      aria-label={fixture.name}
      onContextMenu={(e) =>
        openMenu(
          e,
          [
            { label: own ? "Change its own effect" : "Give it an effect of its own", icon: <Plus size={13} />, action: () => setPicking(true) },
            { label: "Find it on the stage", icon: <Grid3x3 size={13} />, action: () => stageModePref.set("stage") },
          ],
          fixture.name,
        )
      }
    >
      <header className="light-card-head">
        <strong>{fixture.name}</strong>
        <span className="hint">
          {size}
          {group ? ` · ${group.name}` : ""}
        </span>
      </header>
      <canvas ref={canvas} className={`light-live ${fixture.shape.type}`} aria-label={`What ${fixture.name} shows now`} />
      {layers.length === 0 ? (
        <p className="hint">Nothing plays on this light.</p>
      ) : (
        <ul className="light-effects">
          {[...layers].reverse().map((layer) => (
            <EffectRow key={layer.id} layer={layer} fixture={fixture} show={show} />
          ))}
        </ul>
      )}
      <div className="light-card-actions" ref={ref}>
        <button type="button" className="text-button" aria-expanded={picking} onClick={() => setPicking(!picking)}>
          <Plus size={13} /> {own ? "Change its own effect" : "Give it an effect of its own"}
        </button>
        {picking && (
          <div className="popover light-picker" role="dialog" aria-label={`An effect for ${fixture.name}`}>
            <EffectPicker onPick={give} current={own?.effect} />
          </div>
        )}
      </div>
    </article>
  );
}

export function LightCards() {
  const show = useShow();
  const canvases = useRef(new Map<number, HTMLCanvasElement>());
  useLivePictures(show, canvases);
  if (!show) return <section className="light-cards" />;
  return (
    <section className="light-cards" aria-label="Lights one by one">
      <div className="light-cards-head">
        <p className="hint">
          Each light with what it shows now and the effects that play on it. "Give it an effect of its own" puts one on
          this light alone, over the rest. Right-click for more.
        </p>
        <button type="button" className="chip" onClick={() => stageModePref.set("stage")}>
          <Grid3x3 size={13} /> Back to the stage
        </button>
      </div>
      {show.fixtures.length === 0 ? (
        <p className="hint">No lights yet. Add one on the left.</p>
      ) : (
        <div className="light-grid">
          {show.fixtures.map((f) => (
            <LightCard
              key={f.id}
              fixture={f}
              show={show}
              canvas={(el) => {
                if (el) canvases.current.set(f.id, el);
                else canvases.current.delete(f.id);
              }}
            />
          ))}
        </div>
      )}
    </section>
  );
}
