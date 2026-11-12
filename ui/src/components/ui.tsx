// Small shared controls.

import { useEffect, useRef, useState, type ReactNode } from "react";

interface SliderProps {
  value: number;
  min: number;
  max: number;
  step?: number;
  label: string;
  /** Called continuously while dragging. */
  onChange: (value: number) => void;
  format?: (value: number) => string;
  /** Hide the label and value, for sliders inside a row that names them. */
  bare?: boolean;
  /** Something small to sit beside the value, such as a button. */
  extra?: ReactNode;
}

/**
 * A range slider that keeps its own value while being dragged, so the
 * handle follows the pointer exactly instead of waiting for the engine to
 * echo each change back.
 */
export function Slider({ value, min, max, step = 0.01, label, onChange, format, bare, extra }: SliderProps) {
  // The value the user last set, shown until the engine reports it back.
  // Dropping it the moment the pointer or key is released would snap the
  // handle to the engine's older value for a frame, and rapid key presses
  // would each start again from that older value.
  const [local, setLocal] = useState<number | null>(null);
  const settle = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => {
    if (local !== null && Math.abs(value - local) <= step / 2) setLocal(null);
  }, [value, local, step]);
  useEffect(() => () => clearTimeout(settle.current), []);

  const shown = local ?? value;
  const text = format ? format(shown) : step >= 1 ? shown.toFixed(0) : shown.toFixed(2);
  const input = (
    <input
      type="range"
      className="slider"
      aria-label={label}
      min={min}
      max={max}
      step={step}
      value={shown}
      style={{ "--fill": `${((shown - min) / (max - min)) * 100}%` } as React.CSSProperties}
      onChange={(e) => {
        const next = Number(e.target.value);
        setLocal(next);
        onChange(next);
        // If the engine settles on something else (it clamps, or the
        // change was refused), fall back to its value.
        clearTimeout(settle.current);
        settle.current = setTimeout(() => setLocal(null), 600);
      }}
    />
  );
  if (bare) return input;
  if (extra) {
    // Not a label: a button inside one would be part of the slider's name.
    return (
      <div className="field">
        <span className="field-head">
          <span>{label}</span>
          <span className="field-extra">
            {extra}
            <output>{text}</output>
          </span>
        </span>
        {input}
      </div>
    );
  }
  return (
    <label className="field">
      <span className="field-head">
        <span>{label}</span>
        <output>{text}</output>
      </span>
      {input}
    </label>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      title={label}
      disabled={disabled}
      className={`switch ${checked ? "on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

export function IconButton({
  label,
  onClick,
  children,
  active,
  danger,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  active?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      className={`icon-button ${active ? "active" : ""} ${danger ? "danger" : ""}`}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/**
 * A button that asks once more before doing something that cannot be
 * undone. The first click arms it; a second click within a few seconds
 * confirms. The confirm target is the same spot, so no pointer travel.
 */
export function ConfirmButton({
  label,
  confirmLabel,
  onConfirm,
  children,
}: {
  label: string;
  confirmLabel: string;
  onConfirm: () => void;
  children: ReactNode;
}) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(timer);
  }, [armed]);
  return (
    <button
      type="button"
      className={`icon-button danger ${armed ? "armed" : ""}`}
      aria-label={armed ? confirmLabel : label}
      title={armed ? confirmLabel : label}
      onClick={() => {
        if (armed) onConfirm();
        setArmed(!armed);
      }}
    >
      {armed ? <span className="armed-text">{confirmLabel}</span> : children}
    </button>
  );
}

/** Closes something when a click lands outside it or Escape is pressed. */
export function useDismiss(open: boolean, close: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, close]);
  return ref;
}

/**
 * A text field for a name. It is saved when the field loses focus or
 * Enter is pressed, and Escape puts the old name back.
 */
export function NameField({
  value,
  onCommit,
  label = "Name",
  placeholder,
  autoSelect,
  allowEmpty,
}: {
  value: string;
  onCommit: (name: string) => void;
  label?: string;
  placeholder?: string;
  /** Select the text when the field appears, ready to be typed over. */
  autoSelect?: boolean;
  allowEmpty?: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (autoSelect) inputRef.current?.select();
    // Only when the field first appears.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const commit = () => {
    const next = draft?.trim();
    if (next !== undefined && next !== value && (next || allowEmpty)) onCommit(next);
    setDraft(null);
  };
  return (
    <label className="field">
      <span className="field-head">
        <span>{label}</span>
      </span>
      <input
        ref={inputRef}
        type="text"
        maxLength={80}
        placeholder={placeholder}
        value={draft ?? value}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") setDraft(null);
        }}
      />
    </label>
  );
}
