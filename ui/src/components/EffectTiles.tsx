// The list effects are chosen from: square tiles with a small moving
// picture of each, in sections that fold, with a place at the top that
// says what the effect under the pointer does.

import { Bookmark, ChevronRight, Info, Search, Volume2, Zap } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useHello, useLibrary, type EffectInfo, type SavedPattern } from "../engine";

/** The frames the engine drew of one effect. */
interface Preview {
  side: number;
  frames: number;
  rate: number;
  data: Uint8Array;
  /** The frame with the most light in it, shown while still. */
  best: number;
}

const previews = new Map<string, Promise<Preview | null>>();

function loadPreview(effect: string): Promise<Preview | null> {
  let made = previews.get(effect);
  if (!made) {
    made = fetch(`/api/preview/${encodeURIComponent(effect)}`)
      .then(async (response) => {
        if (!response.ok) return null;
        const bytes = new Uint8Array(await response.arrayBuffer());
        const [side, , frames, rate] = bytes;
        if (!side || !frames || !rate || bytes.length < 4 + side * side * 3 * frames) return null;
        const size = side * side * 3;
        let best = 0;
        let most = -1;
        for (let f = 0; f < frames; f++) {
          let sum = 0;
          for (let i = 4 + f * size; i < 4 + (f + 1) * size; i += 7) sum += bytes[i]!;
          if (sum > most) [most, best] = [sum, f];
        }
        return { side, frames, rate, data: bytes, best };
      })
      .catch(() => null);
    previews.set(effect, made);
  }
  return made;
}

/** One effect's picture: still, or moving while `playing`. */
export function EffectThumb({ effect, playing, className }: { effect: string; playing: boolean; className?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  useEffect(() => {
    let alive = true;
    void loadPreview(effect).then((p) => alive && setPreview(p));
    return () => {
      alive = false;
    };
  }, [effect]);
  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx || !preview) return;
    const { side, frames, rate, data } = preview;
    el.width = side;
    el.height = side;
    const image = ctx.createImageData(side, side);
    const draw = (frame: number) => {
      const from = 4 + frame * side * side * 3;
      for (let i = 0; i < side * side; i++) {
        image.data[i * 4] = data[from + i * 3]!;
        image.data[i * 4 + 1] = data[from + i * 3 + 1]!;
        image.data[i * 4 + 2] = data[from + i * 3 + 2]!;
        image.data[i * 4 + 3] = 255;
      }
      ctx.putImageData(image, 0, 0);
    };
    if (!playing) {
      draw(preview.best);
      return;
    }
    const began = performance.now();
    let last = -1;
    let raf = 0;
    const tick = (now: number) => {
      const frame = Math.floor(((now - began) / 1000) * rate) % frames;
      if (frame !== last) draw((last = frame));
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [preview, playing]);
  return <canvas ref={canvas} className={`effect-thumb ${className ?? ""}`} aria-hidden />;
}

/** How many tiles a section shows until it is asked for all of them. */
const FEW = 6;

interface Remembered {
  folded: string[];
  all: string[];
}

function remembered(): Remembered {
  try {
    const kept = JSON.parse(localStorage.getItem("darkrgb.picker") ?? "{}") as Partial<Remembered>;
    return { folded: kept.folded ?? [], all: kept.all ?? [] };
  } catch {
    return { folded: [], all: [] };
  }
}

function toggled(list: string[], item: string): string[] {
  return list.includes(item) ? list.filter((i) => i !== item) : [...list, item];
}

interface Tile {
  key: string;
  name: string;
  /** The effect whose picture stands for it. */
  effect: string;
  description: string;
  info?: EffectInfo;
  kept: boolean;
  pick: () => void;
}

export function EffectPicker({
  onPick,
  onUse,
  current,
}: {
  onPick: (id: string) => void;
  /** Given when something kept can be added here: called with its id. */
  onUse?: (pattern: number) => void;
  current?: string;
}) {
  const hello = useHello();
  const library = useLibrary();
  const [search, setSearch] = useState("");
  const [hover, setHover] = useState<string | null>(null);
  const [pinned, setPinned] = useState<string | null>(null);
  const [memory, setMemory] = useState(remembered);
  const remember = (next: Remembered) => {
    setMemory(next);
    try {
      localStorage.setItem("darkrgb.picker", JSON.stringify(next));
    } catch {
      // Not being able to remember which sections are folded is no loss.
    }
  };

  const sections = useMemo(() => {
    const effects = hello?.effects ?? [];
    const describe = (pattern: SavedPattern) =>
      pattern.look
        ? `A look of ${pattern.layers.length} effects. Added as a scene.`
        : (effects.find((e) => e.id === pattern.layers[0]?.effect)?.name ?? "An effect") + ", with your settings.";
    const out: { name: string; tiles: Tile[] }[] = [];
    if (onUse && library.patterns.length > 0) {
      out.push({
        name: "Kept by you",
        tiles: library.patterns.map((pattern) => ({
          key: `kept:${pattern.id}`,
          name: pattern.name,
          effect: pattern.layers[0]?.effect ?? "",
          description: describe(pattern),
          kept: true,
          pick: () => onUse(pattern.id),
        })),
      });
    }
    for (const category of new Set(effects.map((e) => e.category))) {
      out.push({
        name: category,
        tiles: effects
          .filter((e) => e.category === category)
          .map((effect) => ({
            key: effect.id,
            name: effect.name,
            effect: effect.id,
            description: effect.description,
            info: effect,
            kept: false,
            pick: () => onPick(effect.id),
          })),
      });
    }
    return out;
  }, [hello, library.patterns, onPick, onUse]);

  const words = search.trim().toLowerCase();
  const matches = (tile: Tile) => words === "" || `${tile.name} ${tile.description}`.toLowerCase().includes(words);
  const all = sections.flatMap((s) => s.tiles);
  const shown = all.find((t) => t.key === (hover ?? pinned ?? current));
  const found = all.filter(matches).length;

  return (
    <div className="effect-picker">
      <div className="picker-top">
        <label className="picker-search">
          <Search size={13} aria-hidden />
          <input
            type="search"
            placeholder="Find an effect"
            aria-label="Find an effect"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => e.stopPropagation()}
          />
        </label>
        <div className="picker-detail" aria-live="polite">
          {shown ? (
            <>
              <EffectThumb effect={shown.effect} playing className="large" />
              <div className="picker-words">
                <strong>{shown.name}</strong>
                <span>{shown.description}</span>
                {shown.info && (shown.info.audio || shown.info.flash !== "none") && (
                  <span className="picker-badges">
                    {shown.info.audio && (
                      <span className="badge">
                        <Volume2 size={11} aria-hidden /> Follows sound
                      </span>
                    )}
                    {shown.info.flash !== "none" && (
                      <span className="badge warn">
                        <Zap size={11} aria-hidden /> Flashes
                      </span>
                    )}
                  </span>
                )}
              </div>
            </>
          ) : (
            <span className="hint">Point at an effect, or press its ⓘ, to see it move and read what it does.</span>
          )}
        </div>
      </div>
      {words !== "" && found === 0 && <p className="hint">Nothing called that.</p>}
      {sections.map((section) => {
        const tiles = section.tiles.filter(matches);
        if (tiles.length === 0) return null;
        const folded = words === "" && memory.folded.includes(section.name);
        const everything = words !== "" || memory.all.includes(section.name) || tiles.length <= FEW;
        // The one in use is always among those shown.
        const few = tiles.slice(0, FEW);
        const chosen = tiles.find((t) => t.key === current);
        if (!everything && chosen && !few.includes(chosen)) few[FEW - 1] = chosen;
        return (
          <section key={section.name} className="tile-section">
            <div className="tile-section-head">
              <button
                type="button"
                className="section-toggle"
                aria-expanded={!folded}
                onClick={() => remember({ ...memory, folded: toggled(memory.folded, section.name) })}
              >
                <ChevronRight size={13} className="chevron" aria-hidden />
                {section.name}
                <span className="count">{tiles.length}</span>
              </button>
              {!folded && words === "" && tiles.length > FEW && (
                <button
                  type="button"
                  className="text-button"
                  onClick={() => remember({ ...memory, all: toggled(memory.all, section.name) })}
                >
                  {everything ? "Show fewer" : `Show all ${tiles.length}`}
                </button>
              )}
            </div>
            {!folded && (
              <div className="tile-grid">
                {(everything ? tiles : few).map((tile) => (
                  <div
                    key={tile.key}
                    className={`effect-tile${tile.key === current ? " selected" : ""}${tile.kept ? " kept" : ""}`}
                    onPointerEnter={() => setHover(tile.key)}
                    onPointerLeave={() => setHover((h) => (h === tile.key ? null : h))}
                  >
                    <button
                      type="button"
                      className="tile-main"
                      title={tile.description}
                      onFocus={() => setHover(tile.key)}
                      onBlur={() => setHover((h) => (h === tile.key ? null : h))}
                      onClick={tile.pick}
                    >
                      <EffectThumb effect={tile.effect} playing={hover === tile.key} />
                      <span className="tile-name">
                        {tile.kept && <Bookmark size={11} aria-hidden />}
                        {tile.name}
                      </span>
                    </button>
                    <button
                      type="button"
                      className="tile-info"
                      aria-label={`About ${tile.name}`}
                      aria-pressed={pinned === tile.key}
                      onClick={() => setPinned(pinned === tile.key ? null : tile.key)}
                    >
                      <Info size={13} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
