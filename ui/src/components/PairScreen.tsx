// Shown on a phone or another computer before it is let in: the engine
// asks for the six digits displayed on the computer it runs on.

import { useState } from "react";
import { engine } from "../engine";

export function PairScreen() {
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const digits = code.replace(/\D/g, "");
  const submit = async () => {
    if (digits.length !== 6 || busy) return;
    setBusy(true);
    const result = await engine.pair(digits);
    setBusy(false);
    if (!result.ok) {
      setError(result.error ?? "That did not work.");
      setCode("");
    }
  };
  return (
    <div className="offline pair" role="dialog" aria-modal="true" aria-label="Pair this device">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <h1>Enter the code</h1>
        <p>
          This is darkRGB running on another computer. To control its lights from this device, type the six
          digits it shows in Settings, under General.
        </p>
        <input
          className="pair-code"
          type="text"
          inputMode="numeric"
          autoComplete="one-time-code"
          aria-label="Pairing code"
          placeholder="000 000"
          maxLength={9}
          autoFocus
          value={code}
          onChange={(e) => {
            setCode(e.target.value);
            setError(null);
          }}
        />
        {error && <p className="callout error">{error}</p>}
        <button type="submit" className="button" disabled={digits.length !== 6 || busy}>
          Connect
        </button>
      </form>
    </div>
  );
}
