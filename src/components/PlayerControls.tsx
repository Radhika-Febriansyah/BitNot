// Playback controls shared by the player bar and the Now Playing page, so
// both always show the same state and behave the same way.

import { useState, type CSSProperties } from "react";
import { t, type MessageKey } from "../i18n";
import { formatDuration, type Repeat } from "../lib/library";
import { shortcutHint, type ShortcutAction } from "../lib/shortcuts";
import { useLibrary, usePosition } from "../state/LibraryContext";
import { Icon } from "./Icon";

const NEXT_REPEAT: Record<Repeat, Repeat> = { off: "all", all: "one", one: "off" };
const REPEAT_LABEL: Record<Repeat, MessageKey> = {
  off: "player.repeatOff",
  all: "player.repeatAll",
  one: "player.repeatOne",
};

/** Tooltip text with the keyboard shortcut, e.g. "Next (Ctrl+→)". */
export function hinted(label: string, action: ShortcutAction): string {
  return t("player.withShortcut", { label, keys: shortcutHint(action) });
}

export function fill(value: number, max: number): CSSProperties {
  const percent = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return { "--fill": `${percent}%` } as CSSProperties;
}

/** Whether the transport can act, and what to say when it cannot. */
export function usePlayable(): { canPlay: boolean; hint: string } {
  const { track, nextUp, outputAvailable } = useLibrary().playback;
  return {
    canPlay: outputAvailable && (track !== null || nextUp.length > 0),
    hint: outputAvailable ? t("player.nothingYet") : t("player.noDevice"),
  };
}

/** How often the seek bar re-renders, in milliseconds of track time. */
export function seekGranularity(durationMs: number, reduceMotion = prefersStillPosition()): number {
  if (reduceMotion || durationMs <= 0) return 1000;
  return Math.min(1000, Math.max(16, Math.round(durationMs / 1000)));
}

function prefersStillPosition(): boolean {
  return typeof document !== "undefined" && document.documentElement.dataset.motion === "reduce";
}

/** Keyboard steps on the seek bar, kept as they were before the bar moved smoothly. */
const SEEK_KEY_STEP_MS = 1000;
const SEEK_PAGE_STEP_MS = 10_000;

/**
 * Where a key press on the seek bar moves to, or null for keys it leaves alone.
 * The slider itself has 1 ms steps so its thumb follows the smooth position;
 * these keys keep the 1-second (and 10-second page) steps it always had.
 */
export function seekKeyTarget(key: string, currentMs: number, durationMs: number): number | null {
  const clamp = (ms: number) => Math.min(Math.max(0, ms), Math.max(0, durationMs));
  switch (key) {
    case "ArrowRight":
    case "ArrowUp":
      return clamp(currentMs + SEEK_KEY_STEP_MS);
    case "ArrowLeft":
    case "ArrowDown":
      return clamp(currentMs - SEEK_KEY_STEP_MS);
    case "PageUp":
      return clamp(currentMs + SEEK_PAGE_STEP_MS);
    case "PageDown":
      return clamp(currentMs - SEEK_PAGE_STEP_MS);
    case "Home":
      return 0;
    case "End":
      return clamp(durationMs);
    default:
      return null;
  }
}

/** Seek bar. Dragging previews the position and seeks on release. */
export function SeekBar({ className = "player-seek" }: { className?: string }) {
  const { seek, playback } = useLibrary();
  const durationMs = playback.track?.durationMs ?? 0;
  const disabled = !playback.track || !playback.outputAvailable;
  // About a thousand steps across the track, so the bar moves smoothly (under a
  // pixel per step) without re-rendering on every animation frame. With Reduce
  // Motion it moves once a second.
  const positionMs = usePosition(seekGranularity(durationMs));
  const [dragging, setDragging] = useState<number | null>(null);
  const shown = Math.min(dragging ?? positionMs, durationMs || positionMs);
  const commit = () => {
    if (dragging !== null) {
      seek(dragging);
      setDragging(null);
    }
  };

  return (
    <div className={className}>
      <span className="num">{formatDuration(shown)}</span>
      <input
        type="range"
        className="slider"
        aria-label={t("player.position")}
        aria-valuetext={t("player.positionValue", {
          position: formatDuration(shown),
          duration: formatDuration(durationMs),
        })}
        min={0}
        max={Math.max(1, durationMs)}
        step={1}
        value={shown}
        disabled={disabled}
        style={fill(shown, durationMs)}
        onChange={(event) => setDragging(Number(event.target.value))}
        onKeyDown={(event) => {
          // Modified arrows are app shortcuts (next, previous, seek, volume).
          if (event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) return;
          const target = seekKeyTarget(event.key, shown, durationMs);
          if (target === null) return;
          event.preventDefault();
          setDragging(target);
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="num">{formatDuration(durationMs)}</span>
    </div>
  );
}

/** Shuffle, previous, play/pause, next, repeat. */
export function Transport({ large = false }: { large?: boolean }) {
  const { playback, toggle, next, previous, setShuffle, setRepeat } = useLibrary();
  const { playing, shuffle, repeat } = playback;
  const { canPlay, hint } = usePlayable();
  const repeatText = t(REPEAT_LABEL[repeat]);
  const iconSize = large ? 1.5 : 1.25;

  return (
    <div className={large ? "player-transport large" : "player-transport"}>
      <button
        className={shuffle ? "transport on" : "transport"}
        aria-label={t("player.shuffle")}
        aria-pressed={shuffle}
        title={shuffle ? t("player.shuffleOn") : t("player.shuffleOff")}
        onClick={() => setShuffle(!shuffle)}
      >
        <Icon name="shuffle" size={iconSize} />
      </button>
      <button
        className="transport"
        aria-label={t("player.previous")}
        title={canPlay ? hinted(t("player.previous"), "previous") : hint}
        disabled={!canPlay}
        onClick={previous}
      >
        <Icon name="previous" size={iconSize} />
      </button>
      <button
        className="transport primary"
        aria-label={playing ? t("player.pause") : t("player.play")}
        title={canPlay ? hinted(playing ? t("player.pause") : t("player.play"), "togglePlay") : hint}
        disabled={!canPlay}
        onClick={toggle}
      >
        <Icon name={playing ? "pause" : "play"} size={iconSize} />
      </button>
      <button
        className="transport"
        aria-label={t("player.next")}
        title={canPlay ? hinted(t("player.next"), "next") : hint}
        disabled={!canPlay}
        onClick={next}
      >
        <Icon name="next" size={iconSize} />
      </button>
      <button
        className={repeat === "off" ? "transport" : "transport on"}
        aria-label={t("player.repeatLabel", { state: repeatText })}
        title={repeatText}
        onClick={() => setRepeat(NEXT_REPEAT[repeat])}
      >
        <Icon name="repeat" size={iconSize} />
        {repeat === "one" && <span className="repeat-one">1</span>}
      </button>
    </div>
  );
}

/** Mute button and volume slider. */
export function VolumeControl({ className = "player-volume" }: { className?: string }) {
  const { volume, setVolume, muted, toggleMute } = useLibrary();
  return (
    <div className={className} data-muted={muted}>
      <button
        className="icon-btn"
        aria-label={muted ? t("player.unmute") : t("player.mute")}
        aria-pressed={muted}
        title={hinted(muted ? t("player.unmute") : t("player.mute"), "mute")}
        onClick={toggleMute}
      >
        <Icon name={muted ? "volume-x" : "volume"} size={1.25} />
      </button>
      <input
        type="range"
        className="slider"
        aria-label={t("player.volume")}
        aria-valuetext={muted ? `${t("player.mute")}, ${volume}%` : `${volume}%`}
        min={0}
        max={100}
        value={volume}
        style={fill(muted ? 0 : volume, 100)}
        onChange={(event) => setVolume(Number(event.target.value))}
      />
    </div>
  );
}
