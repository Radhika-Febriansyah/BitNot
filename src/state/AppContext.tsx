import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { api, errorMessage } from "../lib/api";
import {
  canGoBack,
  canGoForward,
  currentRoute,
  goBack,
  goForward,
  initialNav,
  navigate,
  navStartingAt,
  parseRoute,
  serializeRoute,
  type NavState,
  type Route,
} from "../lib/router";
import type { Bootstrap, Profile, Settings } from "../lib/types";

export interface Toast {
  id: number;
  message: string;
  kind: "info" | "error";
}

interface AppContextValue {
  boot: Bootstrap;
  settings: Settings;
  activeProfile: Profile;
  /** Saves a partial settings change. The UI updates at once and reverts on failure. */
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  /** Runs a core operation that returns fresh app state, reporting any failure. */
  run: (operation: () => Promise<Bootstrap>) => Promise<boolean>;
  notify: (message: string, kind?: Toast["kind"]) => void;
  toasts: Toast[];
  route: Route;
  go: (route: Route) => void;
  back: () => void;
  forward: () => void;
  canBack: boolean;
  canForward: boolean;
}

const AppContext = createContext<AppContextValue | null>(null);

type NavAction = { type: "go"; route: Route } | { type: "back" } | { type: "forward" } | { type: "reset" };

function navReducer(state: NavState, action: NavAction): NavState {
  switch (action.type) {
    case "go":
      return navigate(state, action.route);
    case "back":
      return goBack(state);
    case "forward":
      return goForward(state);
    case "reset":
      return initialNav;
  }
}

const TOAST_MS = 5000;

interface ProviderProps {
  boot: Bootstrap;
  onBoot: (boot: Bootstrap) => void;
  children: ReactNode;
}

export function AppProvider({ boot, onBoot, children }: ProviderProps) {
  // Open on the page that was showing last time (crash and restart recovery).
  const [nav, dispatch] = useReducer(navReducer, boot.settings.lastPage, (saved) =>
    navStartingAt(parseRoute(saved)),
  );
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextToastId = useRef(1);
  const bootRef = useRef(boot);
  bootRef.current = boot;

  const notify = useCallback((message: string, kind: Toast["kind"] = "info") => {
    const id = nextToastId.current++;
    setToasts((current) => [...current.slice(-2), { id, message, kind }]);
    window.setTimeout(() => {
      setToasts((current) => current.filter((toast) => toast.id !== id));
    }, TOAST_MS);
  }, []);

  const updateSettings = useCallback(
    async (patch: Partial<Settings>) => {
      const before = bootRef.current;
      onBoot({ ...before, settings: { ...before.settings, ...patch } });
      try {
        const saved = await api.updateSettings(patch);
        onBoot({ ...bootRef.current, settings: saved });
      } catch (error) {
        onBoot({ ...bootRef.current, settings: before.settings });
        notify(errorMessage(error), "error");
      }
    },
    [notify, onBoot],
  );

  const run = useCallback(
    async (operation: () => Promise<Bootstrap>) => {
      try {
        const next = await operation();
        if (next.activeProfileId !== bootRef.current.activeProfileId) {
          dispatch({ type: "reset" });
        }
        onBoot(next);
        return true;
      } catch (error) {
        notify(errorMessage(error), "error");
        return false;
      }
    },
    [notify, onBoot],
  );

  // Remember the current page. Written a moment after navigation settles, and
  // directly to the core so it does not re-render the app.
  const page = serializeRoute(currentRoute(nav));
  const setupComplete = boot.setupComplete;
  useEffect(() => {
    if (!setupComplete || page === bootRef.current.settings.lastPage) return;
    const timer = window.setTimeout(() => {
      api.updateSettings({ lastPage: page }).catch(() => undefined);
    }, 600);
    return () => window.clearTimeout(timer);
  }, [page, setupComplete]);

  const value = useMemo<AppContextValue>(() => {
    const activeProfile =
      boot.profiles.find((profile) => profile.id === boot.activeProfileId) ?? boot.profiles[0];
    return {
      boot,
      settings: boot.settings,
      activeProfile,
      updateSettings,
      run,
      notify,
      toasts,
      route: currentRoute(nav),
      go: (route) => dispatch({ type: "go", route }),
      back: () => dispatch({ type: "back" }),
      forward: () => dispatch({ type: "forward" }),
      canBack: canGoBack(nav),
      canForward: canGoForward(nav),
    };
  }, [boot, nav, notify, run, toasts, updateSettings]);

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApp(): AppContextValue {
  const value = useContext(AppContext);
  if (!value) throw new Error("useApp must be used inside AppProvider");
  return value;
}
