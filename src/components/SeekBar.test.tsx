// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useEffect, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_PLAYBACK, type PlaybackState, type Track } from "../lib/library";
import { PlaybackClock, type ClockEnvironment } from "../lib/playbackClock";
import { seekGranularity, seekKeyTarget, SeekBar } from "./PlayerControls";

// A deterministic clock shared by both seek bars, as LibraryProvider shares one.
let time = 0;
let frames: (() => void)[] = [];
const env: ClockEnvironment = {
  now: () => time,
  isHidden: () => false,
  requestFrame: (callback) => frames.push(callback),
  cancelFrame: () => undefined,
};
let clock: PlaybackClock;

function advance(ms: number) {
  for (let left = ms; left > 0; left -= 16) {
    time += Math.min(16, left);
    const due = frames;
    frames = [];
    act(() => due.forEach((callback) => callback()));
  }
}

const track = { id: 1, title: "Night Bus", durationMs: 200_000 } as Track;
const library = {
  playback: { ...EMPTY_PLAYBACK, track, playing: true } as PlaybackState,
  seek: vi.fn(),
};

vi.mock("../state/LibraryContext", () => ({
  useLibrary: () => library,
  // The real hook's logic, bound to the test clock.
  usePosition: (granularityMs = 1000) => {
    const [position, setPosition] = useState(() => clock.position());
    useEffect(() => clock.subscribe(setPosition, granularityMs), [granularityMs]);
    return position;
  },
}));

beforeEach(() => {
  time = 0;
  frames = [];
  clock = new PlaybackClock(env);
  library.seek.mockClear();
  document.documentElement.dataset.motion = "full";
});
afterEach(cleanup);

const read = () =>
  screen.getAllByRole("slider", { name: "Playback position" }).map((slider) => (slider as HTMLInputElement).value);

describe("SeekBar with the playback clock", () => {
  it("keeps the player bar and Now Playing on the same position", () => {
    clock.update({ trackId: 1, positionMs: 30_000, playing: true, clockMs: 0, durationMs: 200_000 });
    render(
      <>
        <SeekBar />
        <SeekBar className="player-seek np-seek" />
      </>,
    );
    expect(read()).toEqual(["30000", "30000"]);
    advance(500);
    const [bar, nowPlaying] = read();
    expect(bar).toBe(nowPlaying);
    expect(Number(bar)).toBeGreaterThanOrEqual(30_400); // moved within one 200 ms step
    expect(screen.getAllByText("0:30")).toHaveLength(2);

    // Pausing stops both where the core says.
    act(() => clock.update({ trackId: 1, positionMs: 30_510, playing: false, clockMs: 510, durationMs: 200_000 }));
    advance(2_000);
    expect(read()).toEqual(["30510", "30510"]);
  });

  it("lets the user drag while the clock runs, and seeks on release", () => {
    clock.update({ trackId: 1, positionMs: 10_000, playing: true, clockMs: 0, durationMs: 200_000 });
    render(<SeekBar />);
    const slider = screen.getByRole("slider", { name: "Playback position" });
    fireEvent.change(slider, { target: { value: "150000" } });
    advance(400); // the clock keeps running underneath
    expect((slider as HTMLInputElement).value).toBe("150000");
    fireEvent.pointerUp(slider);
    expect(library.seek).toHaveBeenCalledWith(150_000);
  });

  it("keeps one-second keyboard steps while the thumb follows the smooth position", () => {
    expect(seekKeyTarget("ArrowRight", 30_420, 200_000)).toBe(31_420);
    expect(seekKeyTarget("ArrowDown", 30_420, 200_000)).toBe(29_420);
    expect(seekKeyTarget("PageUp", 195_000, 200_000)).toBe(200_000);
    expect(seekKeyTarget("PageDown", 4_000, 200_000)).toBe(0);
    expect(seekKeyTarget("Home", 30_000, 200_000)).toBe(0);
    expect(seekKeyTarget("End", 30_000, 200_000)).toBe(200_000);
    expect(seekKeyTarget("a", 30_000, 200_000)).toBeNull();

    clock.update({ trackId: 1, positionMs: 30_000, playing: false, clockMs: 0, durationMs: 200_000 });
    render(<SeekBar />);
    const slider = screen.getByRole("slider", { name: "Playback position" }) as HTMLInputElement;
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(slider.value).toBe("32000");
    fireEvent.keyUp(slider, { key: "ArrowRight" });
    expect(library.seek).toHaveBeenCalledWith(32_000);
    // Ctrl+Arrow belongs to the app shortcuts, not the slider.
    library.seek.mockClear();
    fireEvent.keyDown(slider, { key: "ArrowRight", ctrlKey: true });
    fireEvent.keyUp(slider, { key: "ArrowRight", ctrlKey: true });
    expect(library.seek).not.toHaveBeenCalled();
  });

  it("moves in about a thousand steps, or once a second with Reduce Motion", () => {
    expect(seekGranularity(200_000, false)).toBe(200);
    expect(seekGranularity(8_000, false)).toBe(16);
    expect(seekGranularity(3_600_000 * 2, false)).toBe(1000);
    expect(seekGranularity(200_000, true)).toBe(1000);
    expect(seekGranularity(0, false)).toBe(1000);
    document.documentElement.dataset.motion = "reduce";
    expect(seekGranularity(200_000)).toBe(1000);
  });
});
