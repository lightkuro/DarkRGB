import { X } from "lucide-react";
import { useEffect } from "react";
import { DevicePanel } from "./components/DevicePanel";
import { LayerPanel } from "./components/LayerPanel";
import { ScenesBar } from "./components/ScenesBar";
import { Stage } from "./components/Stage";
import { TopBar } from "./components/TopBar";
import { engine, useConnected, useShow, useStatus, useToasts } from "./engine";

function Toasts() {
  const toasts = useToasts();
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast ${toast.kind}`}>
          <span>{toast.text}</span>
          <button type="button" aria-label="Dismiss" onClick={() => engine.dismissToast(toast.id)}>
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}

export function App() {
  const connected = useConnected();
  const show = useShow();
  const status = useStatus();
  useEffect(() => engine.connect(), []);

  return (
    <div className="app">
      <TopBar />
      {status?.notice && (
        <div className="notice" role="alert">
          <span>{status.notice}</span>
          <button type="button" className="text-button" onClick={() => engine.send({ type: "dismiss_notice" })}>
            Dismiss
          </button>
        </div>
      )}
      <main className="workspace">
        <DevicePanel />
        <Stage />
        <LayerPanel />
      </main>
      <ScenesBar />
      <Toasts />
      {!connected && (
        <div className="offline" role="alert">
          <div>
            <h1>{show ? "Lost the connection to the engine" : "Connecting to the engine…"}</h1>
            <p>
              {show
                ? "The lights keep whatever they last showed until it is back. Trying again every second."
                : "If this stays here, the darkRGB program is not running."}
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
