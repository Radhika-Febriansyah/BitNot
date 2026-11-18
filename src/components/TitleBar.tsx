import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Wordmark } from "./Logo";

/** Custom title bar: brand, optional controls, drag area, Windows caption buttons. */
export function TitleBar({ children }: { children?: ReactNode }) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      const win = getCurrentWindow();
      const sync = () =>
        win
          .isMaximized()
          .then((value) => !disposed && setMaximized(value))
          .catch(() => undefined);
      void sync();
      win
        .onResized(sync)
        .then((stop) => (disposed ? stop() : (unlisten = stop)))
        .catch(() => undefined);
    } catch {
      // Not running inside the desktop window (for example a browser preview).
    }
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const act = (action: "minimize" | "toggleMaximize" | "close") => () => {
    try {
      void getCurrentWindow()[action]();
    } catch {
      // No desktop window to control.
    }
  };

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand" data-tauri-drag-region>
        <Wordmark size={1.125} />
      </div>
      {children}
      <div className="titlebar-drag" data-tauri-drag-region />
      <div className="caption-buttons">
        <button className="caption-btn" aria-label="Minimize" onClick={act("minimize")}>
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
        <button
          className="caption-btn"
          aria-label={maximized ? "Restore" : "Maximize"}
          onClick={act("toggleMaximize")}
        >
          <svg viewBox="0 0 10 10" width="10" height="10" fill="none" aria-hidden="true">
            {maximized ? (
              <path d="M2.5 2.5v-2h7v7h-2M.5 2.5h7v7h-7z" stroke="currentColor" strokeWidth="1" />
            ) : (
              <rect x=".5" y=".5" width="9" height="9" stroke="currentColor" strokeWidth="1" />
            )}
          </svg>
        </button>
        <button className="caption-btn close" aria-label="Close" onClick={act("close")}>
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            <path d="M0 0l10 10M10 0 0 10" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
      </div>
    </header>
  );
}
