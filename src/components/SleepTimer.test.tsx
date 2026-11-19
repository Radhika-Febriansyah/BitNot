// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_PLAYBACK, type PlaybackState, type SleepStatus, type Track } from "../lib/library";
import type { MenuItem } from "../state/UiContext";
import { sleepCountdown, sleepLabel, SleepTimerButton, useSleepRemaining } from "./SleepTimer";

const library = {
  playback: EMPTY_PLAYBACK as PlaybackState,
  setSleep: vi.fn(),
  cancelSleep: vi.fn(),
};
let menu: MenuItem[] = [];
const ui = { openMenu: vi.fn((items: MenuItem[]) => (menu = items)) };
const app = { notify: vi.fn() };

vi.mock("../state/LibraryContext", () => ({ useLibrary: () => library }));
vi.mock("../state/UiContext", () => ({ useUi: () => ui }));
vi.mock("../state/AppContext", () => ({ useApp: () => app }));

const minutesTimer = (minutes: number, remainingMs: number): SleepStatus => ({
  kind: "minutes",
  minutes,
  remainingMs,
});
const END_OF_TRACK: SleepStatus = { kind: "endOfTrack", minutes: null, remainingMs: null };
const track = { id: 1, title: "Night Bus" } as Track;

type Choice = Exclude<MenuItem, { separator: true }>;
const choice = (label: string) => menu.find((item) => "label" in item && item.label === label) as Choice;
const openMenu = () => fireEvent.click(screen.getByRole("button", { name: /sleep timer/i }));

beforeEach(() => {
  vi.clearAllMocks();
  menu = [];
  library.playback = { ...EMPTY_PLAYBACK, track };
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("sleep timer text", () => {
  it("names the state for the button and its tooltip", () => {
    expect(sleepLabel(null, null)).toBe("Sleep timer");
    expect(sleepLabel(END_OF_TRACK, null)).toBe("Sleep timer: stops after this track");
    expect(sleepLabel(minutesTimer(15, 900_000), 899_000)).toBe("Sleep timer: 14:59 left");
    // Before the countdown has started, the core's figure is used.
    expect(sleepLabel(minutesTimer(60, 3_600_000), null)).toBe("Sleep timer: 1:00:00 left");
  });

  it("rounds up so it never reads 0:00 before the end", () => {
    expect(sleepCountdown(1)).toBe("0:01");
    expect(sleepCountdown(59_001)).toBe("1:00");
    expect(sleepCountdown(0)).toBe("0:00");
  });
});

describe("useSleepRemaining", () => {
  it("counts down from the time the core reported, once a second", () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
    const status = minutesTimer(15, 15 * 60_000);
    const { result, rerender, unmount } = renderHook(({ s }) => useSleepRemaining(s), {
      initialProps: { s: status as SleepStatus | null },
    });
    expect(result.current).toBe(15 * 60_000);
    act(() => vi.advanceTimersByTime(61_000));
    expect(result.current).toBe(15 * 60_000 - 61_000);

    // A fresh state from the core resynchronises the countdown.
    rerender({ s: minutesTimer(15, 600_000) });
    expect(result.current).toBe(600_000);
    act(() => vi.advanceTimersByTime(700_000));
    expect(result.current).toBe(0);

    // Off, or an end-of-track timer: nothing to count, and no timer left running.
    rerender({ s: END_OF_TRACK });
    expect(result.current).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
    rerender({ s: null });
    expect(result.current).toBeNull();
    unmount();
  });
});

describe("SleepTimerButton", () => {
  it("offers the four lengths and stop after the current track", () => {
    render(<SleepTimerButton />);
    const button = screen.getByRole("button", { name: "Sleep timer" });
    expect(button.getAttribute("aria-haspopup")).toBe("menu");
    openMenu();
    const labels = menu.map((item) => ("separator" in item ? "—" : item.label));
    expect(labels).toEqual([
      "15 minutes",
      "30 minutes",
      "45 minutes",
      "60 minutes",
      "Stop after current track",
      "—",
      "Turn off sleep timer",
    ]);
    expect(menu.filter((item) => "checked" in item && item.checked)).toHaveLength(0);
    expect(choice("Turn off sleep timer").disabled).toBe(true);

    for (const minutes of [15, 30, 45, 60]) {
      choice(`${minutes} minutes`).onSelect!();
      expect(library.setSleep).toHaveBeenLastCalledWith({ kind: "minutes", minutes });
      expect(app.notify).toHaveBeenLastCalledWith(
        `Sleep timer set. Playback pauses in ${minutes} minutes.`,
      );
    }
    choice("Stop after current track").onSelect!();
    expect(library.setSleep).toHaveBeenLastCalledWith({ kind: "endOfTrack" });
    expect(app.notify).toHaveBeenLastCalledWith("Sleep timer set. Playback stops when this track ends.");
  });

  it("shows the time left and ticks the chosen length", () => {
    library.playback = { ...EMPTY_PLAYBACK, track, sleep: minutesTimer(30, 29 * 60_000 + 5_000) };
    render(<SleepTimerButton />);
    const button = screen.getByRole("button", { name: "Sleep timer: 29:05 left" });
    expect(button.className).toContain("on");
    expect(button.textContent).toContain("29:05");
    openMenu();
    expect(choice("30 minutes").checked).toBe(true);
    expect(choice("15 minutes").checked).toBe(false);
    expect(choice("Stop after current track").checked).toBe(false);

    choice("Turn off sleep timer").onSelect!();
    expect(library.cancelSleep).toHaveBeenCalledTimes(1);
    expect(app.notify).toHaveBeenLastCalledWith("Sleep timer turned off.");
  });

  it("says when it will stop after this track", () => {
    library.playback = { ...EMPTY_PLAYBACK, track, sleep: END_OF_TRACK };
    render(<SleepTimerButton />);
    const button = screen.getByRole("button", { name: "Sleep timer: stops after this track" });
    expect(button.textContent).toContain("End of track");
    openMenu();
    expect(choice("Stop after current track").checked).toBe(true);
    expect(choice("Turn off sleep timer").disabled).toBe(false);
  });

  it("cannot stop after a track when nothing is loaded", () => {
    library.playback = { ...EMPTY_PLAYBACK };
    render(<SleepTimerButton />);
    openMenu();
    expect(choice("Stop after current track").disabled).toBe(true);
    expect(choice("15 minutes").disabled).toBeFalsy();
  });
});
