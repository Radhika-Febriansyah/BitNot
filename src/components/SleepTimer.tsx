// Sleep timer control (CLAUDE.md §20). The timer itself lives in the Rust
// core (src-tauri/src/sleep.rs); this shows it and lets the user change it.

import { useEffect, useState } from "react";
import { t } from "../i18n";
import { formatDuration, SLEEP_MINUTES, type SleepStatus } from "../lib/library";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";
import { useUi, type MenuItem } from "../state/UiContext";
import { Icon } from "./Icon";

/**
 * Time left on a fixed-length timer, counting down once a second while one is
 * set. The core reports the time left with every state; the countdown runs
 * from that on the browser's monotonic clock, so it needs no polling of the core.
 */
export function useSleepRemaining(status: SleepStatus | null): number | null {
  const remainingMs = status?.kind === "minutes" ? status.remainingMs : null;
  const [left, setLeft] = useState<number | null>(remainingMs);

  useEffect(() => {
    if (remainingMs === null) {
      setLeft(null);
      return;
    }
    const end = performance.now() + remainingMs;
    const update = () => setLeft(Math.max(0, end - performance.now()));
    update();
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [status, remainingMs]);

  return left;
}

/** Remaining time as the button shows it: whole seconds, rounded up, so it never reads 0:00 early. */
export function sleepCountdown(ms: number): string {
  return formatDuration(Math.ceil(ms / 1000) * 1000);
}

/** The button's accessible name and tooltip. */
export function sleepLabel(status: SleepStatus | null, leftMs: number | null): string {
  if (!status) return t("sleep.labelOff");
  if (status.kind === "endOfTrack") return t("sleep.labelEndOfTrack");
  return t("sleep.labelMinutes", { time: sleepCountdown(leftMs ?? status.remainingMs ?? 0) });
}

/** Roughly the menu's height, to open it above the button rather than over it. */
const MENU_HEIGHT_PX = 260;

/** Moon button that opens the sleep timer choices and shows the time left. */
export function SleepTimerButton() {
  const { playback, setSleep, cancelSleep } = useLibrary();
  const { openMenu } = useUi();
  const { notify } = useApp();
  const status = playback.sleep;
  const left = useSleepRemaining(status);
  const label = sleepLabel(status, left);
  const hasTrack = playback.track !== null;

  const open = (anchor: HTMLElement) => {
    const items: MenuItem[] = [
      ...SLEEP_MINUTES.map((minutes) => ({
        label: t("sleep.minutes", { count: minutes }),
        checked: status?.kind === "minutes" && status.minutes === minutes,
        onSelect: () => {
          setSleep({ kind: "minutes", minutes });
          notify(t("sleep.setMinutes", { count: minutes }));
        },
      })),
      {
        label: t("sleep.endOfTrack"),
        checked: status?.kind === "endOfTrack",
        // There must be a track to stop after.
        disabled: !hasTrack,
        onSelect: () => {
          setSleep({ kind: "endOfTrack" });
          notify(t("sleep.setEndOfTrack"));
        },
      },
      { separator: true },
      {
        label: t("sleep.turnOff"),
        disabled: !status,
        onSelect: () => {
          cancelSleep();
          notify(t("sleep.cancelled"));
        },
      },
    ];
    const box = anchor.getBoundingClientRect();
    // Open above the button; the menu keeps itself inside the window.
    openMenu(items, box.left, box.top - MENU_HEIGHT_PX);
  };

  return (
    <button
      className={status ? "icon-btn sleep-btn on" : "icon-btn sleep-btn"}
      aria-label={label}
      aria-haspopup="menu"
      title={label}
      onClick={(event) => open(event.currentTarget)}
    >
      <Icon name="moon" size={1.25} />
      {status && (
        <span className="sleep-left num" aria-hidden="true">
          {status.kind === "minutes" ? sleepCountdown(left ?? 0) : t("sleep.endOfTrackShort")}
        </span>
      )}
    </button>
  );
}
