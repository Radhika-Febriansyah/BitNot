// Dated playback clock: shows the playback position moving smoothly between
// the core's progress readings (about four a second) without ever taking over
// from them. The Rust core stays the authority; this only estimates the
// position between its readings.
//
// Every reading carries `clockMs`, the core's monotonic time when the position
// was read (src-tauri/src/playback.rs, `monotonic_ms`). The clock learns how
// the core's monotonic time lines up with `performance.now()` here. Each
// reading arrives a little late, so the smallest observed difference is the
// best estimate. A reading's age is then known even when it arrives late or
// irregularly, so lateness never adds up to drift.
//
// Rules:
// - Playing: position = reading + time since it was read, capped at the track
//   duration and at MAX_AHEAD_MS past the newest reading.
// - Paused or stopped: the reading's position, unchanged.
// - A reading that disagrees with the estimate by more than SNAP_MS (a seek),
//   a new track, a pause, or a stop is shown at once. A smaller backwards
//   correction is absorbed by holding the shown position until the estimate
//   passes it, so the bar never twitches backwards.
// - A reading older than one already applied is ignored.
// - Animation frames run only while playing, while something is subscribed,
//   while the window is visible, and while the estimate can still move.

export interface ClockReading {
  trackId: number | null;
  positionMs: number;
  playing: boolean;
  /** The core's monotonic time when `positionMs` was read. */
  clockMs: number;
  /** Track length, when known. */
  durationMs: number | null;
}

/** How far past the newest reading the estimate may run if readings stop. */
export const MAX_AHEAD_MS = 1000;
/** A disagreement larger than this is a jump (a seek), shown immediately. */
export const SNAP_MS = 300;
/** How fast the learned clock offset may grow, allowing for slow clock drift. */
export const RELAX_MS_PER_SECOND = 1;

export interface ClockEnvironment {
  now(): number;
  requestFrame(callback: () => void): number;
  cancelFrame(id: number): void;
  isHidden(): boolean;
}

const browserEnvironment: ClockEnvironment = {
  now: () => performance.now(),
  requestFrame: (callback) => window.requestAnimationFrame(callback),
  cancelFrame: (id) => window.cancelAnimationFrame(id),
  isHidden: () => typeof document !== "undefined" && document.visibilityState === "hidden",
};

type Listener = (positionMs: number) => void;

interface Subscription {
  granularityMs: number;
  bucket: number;
}

export class PlaybackClock {
  private reading: ClockReading | null = null;
  /** `now()` minus the core's clock: the smallest difference seen, relaxed slowly. */
  private offset: number | null = null;
  private offsetAt = 0;
  /** The shown position may not fall below this while it is set (see above). */
  private hold: number | null = null;
  private readonly listeners = new Map<Listener, Subscription>();
  private frame: number | null = null;

  constructor(private readonly env: ClockEnvironment = browserEnvironment) {}

  /** Applies an authoritative reading from the core. */
  update(next: ClockReading): void {
    const current = this.reading;
    if (current && next.clockMs < current.clockMs) return; // older than what is shown

    const now = this.env.now();
    const observed = now - next.clockMs;
    if (this.offset === null) {
      this.offset = observed;
    } else {
      const relaxed = this.offset + (RELAX_MS_PER_SECOND * Math.max(0, now - this.offsetAt)) / 1000;
      this.offset = Math.min(observed, relaxed);
    }
    this.offsetAt = now;

    const shown = this.position();
    const expected = current ? this.estimate(current, next.clockMs) : 0;
    const jump =
      !current ||
      next.trackId !== current.trackId ||
      !next.playing ||
      !current.playing ||
      Math.abs(expected - next.positionMs) > SNAP_MS;

    this.reading = next;
    this.hold = jump ? null : shown;
    this.notify(jump);
    this.schedule();
  }

  /** The best estimate of the position now, in milliseconds. */
  position(): number {
    const reading = this.reading;
    if (!reading) return 0;
    const estimate = this.estimate(reading, this.coreNow());
    if (this.hold !== null && estimate < this.hold) return this.cap(reading, this.hold);
    return estimate;
  }

  /**
   * Calls `listener` with the position whenever it moves into a new step of
   * `granularityMs`, and at once after a jump. Returns the unsubscribe function.
   */
  subscribe(listener: Listener, granularityMs: number): () => void {
    const position = this.position();
    this.listeners.set(listener, {
      granularityMs: Math.max(1, granularityMs),
      bucket: Math.floor(position / Math.max(1, granularityMs)),
    });
    listener(position);
    this.schedule();
    return () => {
      this.listeners.delete(listener);
      this.schedule();
    };
  }

  /** Listens for the window being hidden or shown. Returns the detach function. */
  attach(target: Pick<Document, "addEventListener" | "removeEventListener"> = document): () => void {
    const onVisibility = () => {
      if (!this.env.isHidden()) this.notify(true);
      this.schedule();
    };
    target.addEventListener("visibilitychange", onVisibility);
    return () => {
      target.removeEventListener("visibilitychange", onVisibility);
      this.stop();
    };
  }

  /** True while animation frames are requested (for tests and diagnostics). */
  get animating(): boolean {
    return this.frame !== null;
  }

  private coreNow(): number {
    return this.env.now() - (this.offset ?? 0);
  }

  private cap(reading: ClockReading, positionMs: number): number {
    const duration = reading.durationMs;
    return Math.max(0, duration !== null && duration > 0 ? Math.min(positionMs, duration) : positionMs);
  }

  private estimate(reading: ClockReading, coreNow: number): number {
    if (!reading.playing) return this.cap(reading, reading.positionMs);
    const elapsed = Math.min(MAX_AHEAD_MS, Math.max(0, coreNow - reading.clockMs));
    return this.cap(reading, reading.positionMs + elapsed);
  }

  /** Whether the estimate can still move without a new reading. */
  private canAdvance(): boolean {
    const reading = this.reading;
    if (!reading || !reading.playing) return false;
    if (this.coreNow() - reading.clockMs >= MAX_AHEAD_MS) return false;
    const duration = reading.durationMs;
    return !(duration !== null && duration > 0 && this.position() >= duration);
  }

  private notify(force: boolean): void {
    if (!force && this.env.isHidden()) return;
    const position = this.position();
    for (const [listener, subscription] of this.listeners) {
      const bucket = Math.floor(position / subscription.granularityMs);
      if (force || bucket !== subscription.bucket) {
        subscription.bucket = bucket;
        listener(position);
      }
    }
  }

  private schedule(): void {
    const wanted = this.listeners.size > 0 && !this.env.isHidden() && this.canAdvance();
    if (wanted && this.frame === null) {
      this.frame = this.env.requestFrame(this.step);
    } else if (!wanted) {
      this.stop();
    }
  }

  private stop(): void {
    if (this.frame !== null) {
      this.env.cancelFrame(this.frame);
      this.frame = null;
    }
  }

  private readonly step = () => {
    this.frame = null;
    this.notify(false);
    this.schedule();
  };
}
