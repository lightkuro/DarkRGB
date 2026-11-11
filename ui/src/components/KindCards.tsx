// Choosing who a show is for. The choice sets the show up: its starter
// scenes, its pads, how it treats flashing. Nothing is locked by it.

import { Home, PartyPopper, Square, TreePine } from "lucide-react";
import type { ReactNode } from "react";
import { useHello, type Kind } from "../engine";

const ICONS: Record<Kind, ReactNode> = {
  room: <Home size={18} aria-hidden />,
  display: <TreePine size={18} aria-hidden />,
  club: <PartyPopper size={18} aria-hidden />,
  custom: <Square size={18} aria-hidden />,
};

/** What picking a kind does, in a sentence, for under the cards. */
export function kindNote(kind: Kind, looks: number): string {
  switch (kind) {
    case "room":
      return `Starts with ${looks} scenes that change by themselves with the music. The flash limit starts on, and you are asked about it the first time something flashes fast.`;
    case "display":
      return `Starts with ${looks} festive scenes, long and slow to change. The flash limit starts on, and you are asked about it the first time something flashes fast.`;
    case "club":
      return `Starts with ${looks} scenes, pads for a white flash, a strobe, a colour pulse and lightning, and keys for blackout and tempo. The flash limit starts off: strobes run at full speed, which can trigger seizures in people with photosensitive epilepsy.`;
    default:
      return "Starts with one effect and nothing else. Starter scenes can be added later.";
  }
}

export function KindCards({ value, onChange }: { value: Kind; onChange: (kind: Kind) => void }) {
  const kinds = useHello()?.kinds ?? [];
  const chosen = kinds.find((k) => k.id === value);
  return (
    <div className="kinds">
      <div className="kind-cards" role="radiogroup" aria-label="What the show is for">
        {kinds.map((kind) => (
          <button
            key={kind.id}
            type="button"
            role="radio"
            aria-checked={kind.id === value}
            className={`kind-card ${kind.id === value ? "selected" : ""}`}
            onClick={() => onChange(kind.id)}
          >
            {ICONS[kind.id]}
            <span className="kind-name">{kind.name}</span>
            <span className="hint">{kind.description}</span>
          </button>
        ))}
      </div>
      {chosen && <p className="hint">{kindNote(chosen.id, chosen.looks.length)}</p>}
    </div>
  );
}
