import { X } from "lucide-react";
import { useEffect } from "react";
import { DevicePanel } from "./components/DevicePanel";
import { FlashPrompt } from "./components/FlashPrompt";
import { LayerPanel } from "./components/LayerPanel";
import { Library } from "./components/Library";
import { LivePads } from "./components/LivePads";
import { ScenesBar } from "./components/ScenesBar";
import { Settings } from "./components/Settings";
import { Stage } from "./components/Stage";
import { TopBar } from "./components/TopBar";
import { engine, useConnected, useShow, useStatus, useToasts, useView } from "./engine";
import { startKeys } from "./keys";

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
  const view = useView();
  useEffect(() => engine.connect(), []);
  useEffect(() => startKeys(), []);

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
      {view === "live" ? (
        <main className="workspace live">
          <Stage />
          <LivePads />
        </main>
      ) : (
        <main className="workspace">
          <DevicePanel />
          <Stage />
          <LayerPanel />
        </main>
      )}
      {/* In the live view the scenes are pads, so the bar would repeat them. */}
      {view === "edit" && <ScenesBar />}
      <Settings />
      <Library />
      <FlashPrompt />
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
