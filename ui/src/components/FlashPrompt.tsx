// Asks, once for each show, what to do about fast flashing. It appears the
// first time the show has something that flashes fast: an effect made to
// flash is in the stack, or the limit has just softened something. After
// the answer, a small note on the stage is all that is left.

import { ShieldCheck, Zap } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { engine, useHello, useLibrary, useShow, useStatus } from "../engine";

export function FlashPrompt() {
  const show = useShow();
  const status = useStatus();
  const hello = useHello();
  const library = useLibrary();
  // The limit softening something lasts a moment; the question stays.
  const [askedFor, setAskedFor] = useState<string | null>(null);
  const firstButton = useRef<HTMLButtonElement>(null);

  const key = show ? `${library.current ?? ""}:${show.name}` : "";
  const undecided = Boolean(show && !show.flash.decided);
  const flashes =
    show?.layers.some((layer) => {
      const info = hello?.effects.find((e) => e.id === layer.effect);
      return info !== undefined && info.flash !== "none";
    }) ?? false;
  const softened = status?.render.flash_limited ?? false;
  const due = undecided && (flashes || softened);

  useEffect(() => {
    if (due) setAskedFor(key);
  }, [due, key]);
  const open = undecided && askedFor === key;
  useEffect(() => {
    if (open) firstButton.current?.focus();
  }, [open]);

  if (!show || !open) return null;
  const choose = (limit: number) => engine.send({ type: "set_flash", limit, decided: true });
  return (
    <div className="dialog-backdrop">
      <div className="dialog flash-prompt" role="alertdialog" aria-modal="true" aria-labelledby="flash-title">
        <h2 id="flash-title">This show can flash fast</h2>
        <p>
          Flashing faster than about three times a second can trigger seizures in people with photosensitive
          epilepsy. darkRGB can hold this show to three flashes a second: strobes slow down to that, and anything
          else that flashes faster is softened.
        </p>
        <p className="flash-question">Who will see these lights?</p>
        <div className="flash-choices">
          <button type="button" className="flash-choice" ref={firstButton} onClick={() => choose(show.flash.limit || 3)}>
            <ShieldCheck size={20} aria-hidden />
            <span className="flash-choice-title">Keep the limit on</span>
            <span className="hint">
              Homes, displays, and anywhere people have not chosen to stand near strobes.
            </span>
          </button>
          <button type="button" className="flash-choice off" onClick={() => choose(0)}>
            <Zap size={20} aria-hidden />
            <span className="flash-choice-title">Turn the limit off</span>
            <span className="hint">Clubs and concerts, where fast flashing is expected. Strobes run at full speed.</span>
          </button>
        </div>
        <p className="hint">
          This is asked once for each show. Change it whenever you like in Settings, under Lights, or with the
          switch in the Live view.
        </p>
      </div>
    </div>
  );
}
