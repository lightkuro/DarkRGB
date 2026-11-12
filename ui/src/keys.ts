// The keyboard as a lighting console: keys tied to scenes, effects and
// other things in the show. A key press here becomes a `trigger` command;
// the engine decides what it does and remembers what is held.
//
// Keys are matched by where they sit (`KeyA`), so a show works on any
// keyboard layout. What is printed on the key is stored once, when the
// key is assigned, for display.

import { engine, onPage, type Binding } from "./engine";

/** The answer to "which key?": its place, and what is printed on it. */
export interface PickedKey {
  key: string;
  label: string;
}

const PUNCTUATION = [
  "Space",
  "Minus",
  "Equal",
  "BracketLeft",
  "BracketRight",
  "Semicolon",
  "Quote",
  "Comma",
  "Period",
  "Slash",
  "Backslash",
  "Backquote",
];

/** Keys that can be tied to something. The engine checks the same list. */
export function usable(code: string): boolean {
  return /^(Key[A-Z]|Digit[0-9]|Numpad[0-9])$/.test(code) || PUNCTUATION.includes(code);
}

/** What to show on a keycap. Empty for a pad without a key. */
export function keyLabel(binding: Pick<Binding, "key" | "label">): string {
  if (!binding.key) return "";
  if (binding.key === "Space") return "Space";
  if (binding.key.startsWith("Numpad")) return `Num ${binding.key.slice(6)}`;
  if (binding.label) return binding.label.toUpperCase();
  if (binding.key.startsWith("Key")) return binding.key.slice(3);
  if (binding.key.startsWith("Digit")) return binding.key.slice(5);
  return binding.key;
}

/** True while the user is typing text, when keys must be left alone. */
function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target.tagName === "TEXTAREA" || target.tagName === "SELECT") return true;
  if (target instanceof HTMLInputElement) {
    return !["range", "checkbox", "radio", "color", "button", "submit", "file"].includes(target.type);
  }
  return false;
}

let learning: ((picked: PickedKey | null) => void) | null = null;
/** Called when the user presses a key that cannot be used while one is being picked. */
let refused: (() => void) | null = null;

/**
 * Waits for the next key press and returns it, for "press the key you
 * want". Escape, or calling this again, cancels with null.
 */
export function pickKey(onRefused?: () => void): Promise<PickedKey | null> {
  learning?.(null);
  refused = onRefused ?? null;
  return new Promise((resolve) => {
    learning = (picked) => {
      learning = null;
      refused = null;
      resolve(picked);
    };
  });
}

export function cancelPick(): void {
  learning?.(null);
}

/** Binding ids sent "down" and not yet "up", by the key that sent them. */
const down = new Map<string, number[]>();

function release(code: string): void {
  const ids = down.get(code);
  if (!ids) return;
  down.delete(code);
  for (const binding of ids) engine.quiet({ type: "trigger", binding, down: false });
}

function releaseAll(): void {
  for (const code of [...down.keys()]) release(code);
}

function onKeyDown(event: KeyboardEvent): void {
  if (learning) {
    event.preventDefault();
    event.stopPropagation();
    if (event.code === "Escape") learning(null);
    else if (usable(event.code)) {
      const printed = event.key.length === 1 && event.key !== " " ? event.key : "";
      learning({ key: event.code, label: printed });
    } else if (!["ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight"].includes(event.code)) {
      refused?.();
    }
    return;
  }
  // Undo and redo, wherever the pointer is. A text field keeps its own.
  if ((event.ctrlKey || event.metaKey) && !event.altKey && !typing(event.target)) {
    const redo = event.code === "KeyY" || (event.code === "KeyZ" && event.shiftKey);
    if (redo || event.code === "KeyZ") {
      event.preventDefault();
      const able = engine.history.get();
      if (redo ? able.redo : able.undo) engine.send({ type: redo ? "redo" : "undo" });
      return;
    }
  }
  if (event.ctrlKey || event.metaKey || event.altKey || typing(event.target)) return;
  // Only pads on the page that is showing answer to their keys.
  const page = engine.show.get()?.live.page ?? 1;
  const bindings = engine.show.get()?.bindings.filter((b) => b.key === event.code && onPage(b, page)) ?? [];
  if (bindings.length === 0) return;
  // The key belongs to the show now: no scrolling on Space, no clicking
  // whatever button happens to have focus.
  event.preventDefault();
  if (event.repeat || down.has(event.code)) return;
  const ids = bindings.map((b) => b.id);
  down.set(event.code, ids);
  for (const binding of ids) engine.quiet({ type: "trigger", binding, down: true });
}

function onKeyUp(event: KeyboardEvent): void {
  if (down.has(event.code)) {
    event.preventDefault();
    release(event.code);
  }
}

/** Starts listening. Returns a function that stops. */
export function startKeys(): () => void {
  const onHidden = () => {
    if (document.hidden) releaseAll();
  };
  // Capture, so a key being picked never reaches anything else.
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("keyup", onKeyUp, true);
  // A key let go while the window is elsewhere would otherwise stay held.
  window.addEventListener("blur", releaseAll);
  document.addEventListener("visibilitychange", onHidden);
  return () => {
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("keyup", onKeyUp, true);
    window.removeEventListener("blur", releaseAll);
    document.removeEventListener("visibilitychange", onHidden);
    releaseAll();
  };
}
