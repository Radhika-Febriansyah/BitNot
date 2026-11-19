// Keyboard shortcuts (CLAUDE.md §20, plus mute, seek, and the reference).
// Pure functions so the rules are unit-tested; useShortcuts.ts wires them up.
// They work while BitNot has focus. System-wide media keys are Phase 5.

import type { MessageKey } from "../i18n";

export type ShortcutAction =
  | "togglePlay"
  | "next"
  | "previous"
  | "volumeUp"
  | "volumeDown"
  | "mute"
  | "seekForward"
  | "seekBack"
  | "search"
  | "help";

export interface Shortcut {
  action: ShortcutAction;
  /** Keys as shown to the user; alternatives are separate entries. */
  keys: string[][];
  label: MessageKey;
}

/** The reference shown to users, in display order. */
export const SHORTCUTS: Shortcut[] = [
  { action: "togglePlay", keys: [["Space"]], label: "shortcuts.togglePlay" },
  { action: "next", keys: [["Ctrl", "→"]], label: "shortcuts.next" },
  { action: "previous", keys: [["Ctrl", "←"]], label: "shortcuts.previous" },
  { action: "seekForward", keys: [["Ctrl", "Shift", "→"]], label: "shortcuts.seekForward" },
  { action: "seekBack", keys: [["Ctrl", "Shift", "←"]], label: "shortcuts.seekBack" },
  { action: "volumeUp", keys: [["Ctrl", "↑"]], label: "shortcuts.volumeUp" },
  { action: "volumeDown", keys: [["Ctrl", "↓"]], label: "shortcuts.volumeDown" },
  { action: "mute", keys: [["Ctrl", "M"]], label: "shortcuts.mute" },
  { action: "search", keys: [["Ctrl", "F"]], label: "shortcuts.search" },
  { action: "help", keys: [["F1"], ["?"]], label: "shortcuts.help" },
];

/** How much one volume step changes the volume (0–100 scale). */
export const VOLUME_STEP = 5;
/** How far one seek step moves, in milliseconds. */
export const SEEK_STEP_MS = 10_000;

/** Short form for tooltips, e.g. "Ctrl+→". */
export function shortcutHint(action: ShortcutAction): string {
  const shortcut = SHORTCUTS.find((s) => s.action === action);
  return shortcut ? shortcut.keys[0].join("+") : "";
}

export interface KeyInput {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  repeat: boolean;
}

export interface KeyContext {
  /** Focus is in a text field, where keys belong to the text. */
  typing: boolean;
  /** Focus is on a control that Space already activates (button, row, link). */
  onControl: boolean;
  /** A dialog or menu is open: it owns the keyboard. */
  overlayOpen: boolean;
}

/** Actions that make sense to repeat while a key is held down. */
const REPEATABLE: ShortcutAction[] = ["volumeUp", "volumeDown", "seekForward", "seekBack"];

/** Decides which shortcut, if any, a key press means. */
export function shortcutFor(event: KeyInput, context: KeyContext): ShortcutAction | null {
  const action = match(event, context);
  if (action && event.repeat && !REPEATABLE.includes(action)) return null;
  return action;
}

function match(event: KeyInput, context: KeyContext): ShortcutAction | null {
  // Alt and Windows-key combinations belong to the system.
  if (event.altKey || event.metaKey || context.overlayOpen) return null;
  const { key, ctrlKey: ctrl, shiftKey: shift } = event;

  if (ctrl && !shift && (key === "f" || key === "F")) return "search"; // works while typing too
  if (key === "F1" && !ctrl && !shift) return "help";
  if (context.typing) return null;

  if (ctrl && shift) {
    if (key === "ArrowRight") return "seekForward";
    if (key === "ArrowLeft") return "seekBack";
    return null;
  }
  if (ctrl) {
    switch (key) {
      case "ArrowRight":
        return "next";
      case "ArrowLeft":
        return "previous";
      case "ArrowUp":
        return "volumeUp";
      case "ArrowDown":
        return "volumeDown";
      case "m":
      case "M":
        return "mute";
      default:
        return null;
    }
  }
  if (key === "?") return "help";
  if (key === " " && !shift && !context.onControl) return "togglePlay";
  return null;
}

/** Describes where keyboard focus is, for `shortcutFor`. */
export function keyContext(target: EventTarget | null, doc: Document): KeyContext {
  const element = target instanceof Element ? target : null;
  const typing =
    !!element &&
    (element instanceof HTMLTextAreaElement ||
      element instanceof HTMLSelectElement ||
      (element instanceof HTMLInputElement &&
        !["range", "checkbox", "radio", "button", "submit", "reset", "color"].includes(element.type)) ||
      (element as HTMLElement).isContentEditable);
  const onControl =
    !!element &&
    !!element.closest(
      "button, a[href], input, select, summary, [role=button], [role=row], [role=menuitem], [role=radio], [role=checkbox], [role=slider], [role=option], [role=tab], li[tabindex]",
    );
  const overlayOpen = !!doc.querySelector("dialog[open], .menu");
  return { typing, onControl, overlayOpen };
}
