import { useEffect, useState } from "react";
import { Icon } from "./components/Icon";
import { Mark } from "./components/Logo";
import { Shell } from "./components/Shell";
import { TitleBar } from "./components/TitleBar";
import { api, errorMessage } from "./lib/api";
import { setLanguage } from "./i18n";
import { applyAppearance, BRAND_ACCENT, type SystemPreferences } from "./lib/theme";
import type { Bootstrap, Settings } from "./lib/types";
import { NARROW_QUERY, useMediaQuery } from "./lib/useMediaQuery";
import { QuickSetup } from "./pages/QuickSetup";
import { AppProvider, useApp } from "./state/AppContext";
import { LibraryProvider } from "./state/LibraryContext";

type Status =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; boot: Bootstrap };

const DARK_QUERY = "(prefers-color-scheme: dark)";
const MOTION_QUERY = "(prefers-reduced-motion: reduce)";

const FALLBACK_APPEARANCE: Pick<
  Settings,
  "theme" | "accentColor" | "displayScale" | "reduceMotion" | "language"
> = {
  theme: "dark",
  accentColor: BRAND_ACCENT,
  displayScale: 100,
  reduceMotion: "system",
  language: "en",
};

function readSystem(): SystemPreferences {
  return {
    prefersDark: window.matchMedia(DARK_QUERY).matches,
    prefersReducedMotion: window.matchMedia(MOTION_QUERY).matches,
  };
}

/** Keeps the document root in sync with settings and Windows preferences. */
function useAppearance(settings: typeof FALLBACK_APPEARANCE) {
  const [system, setSystem] = useState(readSystem);

  useEffect(() => {
    const queries = [window.matchMedia(DARK_QUERY), window.matchMedia(MOTION_QUERY)];
    const update = () => setSystem(readSystem());
    queries.forEach((query) => query.addEventListener("change", update));
    return () => queries.forEach((query) => query.removeEventListener("change", update));
  }, []);

  const { theme, accentColor, displayScale, reduceMotion, language } = settings;
  useEffect(() => {
    applyAppearance(
      document.documentElement,
      { theme, accentColor, displayScale, reduceMotion, language },
      system,
    );
  }, [theme, accentColor, displayScale, reduceMotion, language, system]);
}

function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className={toast.kind === "error" ? "toast error" : "toast"}>
          {toast.message}
        </div>
      ))}
    </div>
  );
}

/** Title-bar controls that exist only once the main shell is showing. */
function ShellTitleControls() {
  const { settings, updateSettings } = useApp();
  const narrow = useMediaQuery(NARROW_QUERY);
  const rail = narrow || settings.sidebarCollapsed;
  return (
    <button
        className="titlebar-btn"
        aria-label={rail ? "Expand sidebar" : "Collapse sidebar"}
        aria-pressed={!rail}
        title={
          narrow ? "Widen the window to expand the sidebar" : rail ? "Expand sidebar" : "Collapse sidebar"
        }
        disabled={narrow}
        onClick={() => void updateSettings({ sidebarCollapsed: !settings.sidebarCollapsed })}
      >
        <Icon name="panel" size={1} />
      </button>
  );
}

export default function App() {
  const [status, setStatus] = useState<Status>({ state: "loading" });

  useEffect(() => {
    let cancelled = false;
    api
      .bootstrap()
      .then((boot) => !cancelled && setStatus({ state: "ready", boot }))
      .catch((error) => !cancelled && setStatus({ state: "failed", message: errorMessage(error) }));
    return () => {
      cancelled = true;
    };
  }, []);

  useAppearance(status.state === "ready" ? status.boot.settings : FALLBACK_APPEARANCE);

  if (status.state === "ready") {
    const { boot } = status;
    // Messages are looked up as components render, so set the language first.
    setLanguage(boot.settings.language);
    return (
      <AppProvider boot={boot} onBoot={(next) => setStatus({ state: "ready", boot: next })}>
        <LibraryProvider>
          <div className="frame" key={boot.settings.language}>
            <TitleBar>{boot.setupComplete && <ShellTitleControls />}</TitleBar>
            {boot.setupComplete ? <Shell /> : <QuickSetup />}
          </div>
          <Toasts />
        </LibraryProvider>
      </AppProvider>
    );
  }

  return (
    <div className="frame">
      <TitleBar />
      {status.state === "loading" ? (
        <main className="splash" aria-busy="true" aria-label="Starting BitNot">
          <Mark size={4} />
        </main>
      ) : (
        <main className="splash" role="alert">
          <Mark size={3} />
          <h2>BitNot could not start</h2>
          <p className="muted selectable startup-error">{status.message}</p>
          <p className="muted">
            Nothing was overwritten, and your music files are not affected. Close BitNot and try
            again.
          </p>
        </main>
      )}
    </div>
  );
}
