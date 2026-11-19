import { useEffect, useRef } from "react";
import { keyContext, shortcutFor, type ShortcutAction } from "./shortcuts";

export type ShortcutHandlers = Record<ShortcutAction, () => void>;

/**
 * Listens for keyboard shortcuts on the window. Runs after menus and rows
 * have handled their own keys: anything they already handled is skipped.
 */
export function useShortcuts(handlers: ShortcutHandlers): void {
  const latest = useRef(handlers);
  latest.current = handlers;

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const action = shortcutFor(event, keyContext(event.target, document));
      if (!action) return;
      event.preventDefault();
      latest.current[action]();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
