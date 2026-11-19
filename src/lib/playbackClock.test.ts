import { beforeEach, describe, expect, it } from "vitest";
import { MAX_AHEAD_MS, PlaybackClock, SNAP_MS, type ClockEnvironment, type ClockReading } from "./playbackClock";

/** A clock environment driven by the test: time moves only when told to. */
class FakeEnvironment implements ClockEnvironment {
  time = 10_000;
  hidden = false;
  private callbacks = new Map<number, () => void>();
  private nextId = 1;

  now = () => this.time;
  isHidden = () => this.hidden;
  requestFrame = (callback: () => void) => {
    const id = this.nextId++;
    this.callbacks.set(id, callback);
    return id;
  };
  cancelFrame = (id: number) => {
    this.callbacks.delete(id);
  };

  get pendingFrames() {
    return this.callbacks.size;
  }

  /** Moves time on by `ms`, running animation frames every 16 ms along the way. */
  advance(ms: number, frameMs = 16) {
    let left = ms;
    while (left > 0) {
      const step = Math.min(frameMs, left);
      this.time += step;
      left -= step;
      const due = [...this.callbacks.values()];
      this.callbacks.clear();
      due.forEach((callback) => callback());
    }
  }
}

/** The core's monotonic clock in this test runs 9 000 ms behind the window's. */
const CORE_OFFSET = 9_000;

let env: FakeEnvironment;
let clock: PlaybackClock;
let seen: number[];

const coreNow = () => env.time - CORE_OFFSET;

/** A reading taken by the core now, delivered `lateMs` later. */
function deliver(reading: Partial<ClockReading>, lateMs = 0) {
  const full: ClockReading = {
    trackId: 1,
    positionMs: 0,
    playing: true,
    clockMs: coreNow(),
    durationMs: 180_000,
    ...reading,
  };
  env.time += lateMs;
  clock.update(full);
  return full;
}

const last = <T,>(list: T[]): T | undefined => list[list.length - 1];

beforeEach(() => {
  env = new FakeEnvironment();
  clock = new PlaybackClock(env);
  seen = [];
});

describe("PlaybackClock", () => {
  it("advances smoothly between readings while playing", () => {
    deliver({ positionMs: 30_000 });
    clock.subscribe((p) => seen.push(p), 1);
    expect(clock.position()).toBe(30_000);
    env.advance(100);
    expect(clock.position()).toBe(30_100);
    // A listener at 1 ms granularity heard every frame in between.
    expect(seen.length).toBeGreaterThan(5);
    expect(last(seen)).toBe(30_100);
    expect(seen).toEqual([...seen].sort((a, b) => a - b));
  });

  it("stays put while paused and resumes from the reading", () => {
    deliver({ positionMs: 30_000 });
    env.advance(200);
    deliver({ positionMs: 30_200, playing: false });
    clock.subscribe((p) => seen.push(p), 1);
    env.advance(5_000);
    expect(clock.position()).toBe(30_200);
    expect(clock.animating).toBe(false);
    expect(seen).toEqual([30_200]);

    deliver({ positionMs: 30_200, playing: true });
    env.advance(250);
    expect(clock.position()).toBe(30_450);
    expect(clock.animating).toBe(true);
  });

  it("shows seeks forward and back at once", () => {
    deliver({ positionMs: 30_000 });
    clock.subscribe((p) => seen.push(p), 1000);
    env.advance(100);
    deliver({ positionMs: 90_000 });
    expect(clock.position()).toBe(90_000);
    expect(last(seen)).toBe(90_000);
    deliver({ positionMs: 5_000 });
    expect(clock.position()).toBe(5_000);
    expect(last(seen)).toBe(5_000);
  });

  it("absorbs a small correction backwards instead of twitching", () => {
    deliver({ positionMs: 30_000 });
    env.advance(250);
    expect(clock.position()).toBe(30_250);
    // The core says it is 80 ms behind the estimate: smaller than a snap.
    deliver({ positionMs: 30_170 });
    expect(30_250 - 30_170).toBeLessThan(SNAP_MS);
    expect(clock.position()).toBe(30_250);
    env.advance(50);
    expect(clock.position()).toBe(30_250);
    env.advance(100);
    expect(clock.position()).toBe(30_320); // caught up and moving with the reading
  });

  it("snaps on a track change and respects the new duration", () => {
    deliver({ positionMs: 170_000 });
    deliver({ trackId: 2, positionMs: 0, durationMs: 4_000 });
    expect(clock.position()).toBe(0);
    env.advance(250);
    deliver({ trackId: 2, positionMs: 3_900, durationMs: 4_000 });
    env.advance(800);
    expect(clock.position()).toBe(4_000);
    expect(clock.animating).toBe(false);
  });

  it("goes back to zero when playback stops", () => {
    deliver({ positionMs: 60_000 });
    clock.subscribe((p) => seen.push(p), 1000);
    deliver({ trackId: null, positionMs: 0, playing: false, durationMs: null });
    expect(clock.position()).toBe(0);
    expect(last(seen)).toBe(0);
    expect(clock.animating).toBe(false);
  });

  it("does not drift when readings arrive late or irregularly", () => {
    // The first reading arrives promptly, so the offset is learned exactly.
    deliver({ positionMs: 0 });
    let truth = 0;
    const delays = [0, 120, 30, 260, 5, 90, 400, 15, 180, 60, 0, 330];
    for (let round = 0; round < 50; round++) {
      const gap = 150 + (round % 4) * 100; // irregular intervals
      env.advance(gap);
      truth += gap;
      // The core reads the position, then it reaches the window late.
      const late = delays[round % delays.length];
      deliver({ positionMs: truth }, late);
      truth += late;
      // The estimate matches the real position. The only error allowed is
      // the slow offset relaxation (1 ms per second, reset by any prompt
      // reading); counting lateness as playback would be off by hundreds of ms.
      expect(Math.abs(clock.position() - truth)).toBeLessThanOrEqual(5);
    }
  });

  it("ignores a reading older than one already applied", () => {
    const older = { trackId: 1, positionMs: 10_000, playing: true, clockMs: coreNow(), durationMs: 180_000 };
    env.advance(500);
    deliver({ positionMs: 60_000 }); // e.g. the result of a seek
    clock.update(older); // a progress event from before the seek, delivered after it
    expect(clock.position()).toBe(60_000);
  });

  it("never passes the end of the track or runs far ahead of the core", () => {
    deliver({ positionMs: 179_900 });
    env.advance(2_000);
    expect(clock.position()).toBe(180_000);

    // Readings stop (the core is busy): the estimate waits after MAX_AHEAD_MS.
    deliver({ trackId: 3, positionMs: 1_000, durationMs: null });
    clock.subscribe(() => undefined, 1);
    env.advance(10_000);
    expect(clock.position()).toBe(1_000 + MAX_AHEAD_MS);
    expect(clock.animating).toBe(false);
  });

  it("re-renders a subscriber only when its step changes", () => {
    deliver({ positionMs: 0 });
    clock.subscribe((p) => seen.push(p), 180); // a 3-minute track at 1000 steps
    env.advance(1_000);
    // One initial call plus one per 180 ms step: far fewer than 60 frames.
    expect(seen.length).toBe(1 + Math.floor(1_000 / 180));
  });

  it("keeps every subscriber on the same position", () => {
    deliver({ positionMs: 42_000 });
    const bar: number[] = [];
    const nowPlaying: number[] = [];
    clock.subscribe((p) => bar.push(p), 180);
    clock.subscribe((p) => nowPlaying.push(p), 180);
    env.advance(777);
    expect(bar).toEqual(nowPlaying);
    deliver({ positionMs: 100_000 });
    expect(last(bar)).toBe(100_000);
    expect(last(nowPlaying)).toBe(100_000);
  });

  it("requests frames only while playing, subscribed, and visible", () => {
    deliver({ positionMs: 0 });
    expect(clock.animating).toBe(false); // nobody is listening
    const stop = clock.subscribe(() => undefined, 1);
    expect(clock.animating).toBe(true);

    const listeners: Record<string, () => void> = {};
    const target = {
      addEventListener: (_: string, fn: () => void) => (listeners.change = fn),
      removeEventListener: () => delete listeners.change,
    } as unknown as Document;
    const detach = clock.attach(target);
    env.hidden = true;
    listeners.change();
    expect(clock.animating).toBe(false);
    expect(env.pendingFrames).toBe(0);
    env.hidden = false;
    listeners.change();
    expect(clock.animating).toBe(true);

    stop();
    expect(clock.animating).toBe(false);
    expect(env.pendingFrames).toBe(0);
    detach();
    expect(listeners.change).toBeUndefined();
  });

  it("stops when the sleep timer pauses playback", () => {
    deliver({ positionMs: 5_000 });
    clock.subscribe((p) => seen.push(p), 1000);
    env.advance(300);
    // The core paused (sleep timer); its reading says so.
    deliver({ positionMs: 5_300, playing: false });
    env.advance(3_000);
    expect(clock.position()).toBe(5_300);
    expect(clock.animating).toBe(false);
  });
});
