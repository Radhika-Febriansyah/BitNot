import { useState, type CSSProperties } from "react";
import { t, type MessageKey } from "../i18n";
import { formatDuration, type Repeat } from "../lib/library";
import { useLibrary, usePosition } from "../state/LibraryContext";
import { Artwork } from "./Artwork";
import { Icon } from "./Icon";
import { FavoriteButton } from "./TrackTable";

interface PlayerBarProps {
  queueOpen: boolean;
  onToggleQueue: () => void;
}

const NEXT_REPEAT: Record<Repeat, Repeat> = { off: "all", all: "one", one: "off" };
const REPEAT_LABEL: Record<Repeat, MessageKey> = {
  off: "player.repeatOff",
  all: "player.repeatAll",
  one: "player.repeatOne",
};

function fill(value: number, max: number): CSSProperties {
  const percent = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return { "--fill": `${percent}%` } as CSSProperties;
}

/** Seek bar. Dragging previews the position and seeks on release. */
function SeekBar({ durationMs, disabled }: { durationMs: number; disabled: boolean }) {
  const { seek } = useLibrary();
  const positionMs = usePosition();
  const [dragging, setDragging] = useState<number | null>(null);
  const shown = Math.min(dragging ?? positionMs, durationMs || positionMs);
  const commit = () => {
    if (dragging !== null) {
      seek(dragging);
      setDragging(null);
    }
  };

  return (
    <div className="player-seek">
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
        step={1000}
        value={shown}
        disabled={disabled}
        style={fill(shown, durationMs)}
        onChange={(event) => setDragging(Number(event.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="num">{formatDuration(durationMs)}</span>
    </div>
  );
}

/** Persistent bottom bar: current track, transport and progress, queue and volume. */
export function PlayerBar({ queueOpen, onToggleQueue }: PlayerBarProps) {
  const { playback, artworkUrl, toggle, next, previous, setShuffle, setRepeat, volume, setVolume } =
    useLibrary();
  const { track, playing, shuffle, repeat, nextUp, outputAvailable } = playback;
  const canPlay = outputAvailable && (track !== null || nextUp.length > 0);
  const hint = outputAvailable ? t("player.nothingYet") : t("player.noDevice");
  const repeatText = t(REPEAT_LABEL[repeat]);

  return (
    <footer className="player" aria-label={t("player.label")}>
      <div className="player-track">
        <Artwork src={artworkUrl(track?.artwork, true)} size={3.5} />
        <div className="track-text">
          <span className="track-title truncate">{track ? track.title : t("player.nothingPlaying")}</span>
          <span className="track-artist truncate">
            {track ? track.artist : outputAvailable ? t("player.choose") : hint}
          </span>
        </div>
        {track && <FavoriteButton track={track} alwaysVisible />}
      </div>

      <div className="player-center">
        <div className="player-transport">
          <button
            className={shuffle ? "transport on" : "transport"}
            aria-label={t("player.shuffle")}
            aria-pressed={shuffle}
            title={shuffle ? t("player.shuffleOn") : t("player.shuffleOff")}
            onClick={() => setShuffle(!shuffle)}
          >
            <Icon name="shuffle" size={1.25} />
          </button>
          <button
            className="transport"
            aria-label={t("player.previous")}
            title={canPlay ? t("player.previous") : hint}
            disabled={!canPlay}
            onClick={previous}
          >
            <Icon name="previous" size={1.25} />
          </button>
          <button
            className="transport primary"
            aria-label={playing ? t("player.pause") : t("player.play")}
            title={canPlay ? (playing ? t("player.pause") : t("player.play")) : hint}
            disabled={!canPlay}
            onClick={toggle}
          >
            <Icon name={playing ? "pause" : "play"} size={1.25} />
          </button>
          <button
            className="transport"
            aria-label={t("player.next")}
            title={canPlay ? t("player.next") : hint}
            disabled={!canPlay}
            onClick={next}
          >
            <Icon name="next" size={1.25} />
          </button>
          <button
            className={repeat === "off" ? "transport" : "transport on"}
            aria-label={t("player.repeatLabel", { state: repeatText })}
            title={repeatText}
            onClick={() => setRepeat(NEXT_REPEAT[repeat])}
          >
            <Icon name="repeat" size={1.25} />
            {repeat === "one" && <span className="repeat-one">1</span>}
          </button>
        </div>
        <SeekBar durationMs={track?.durationMs ?? 0} disabled={!track || !outputAvailable} />
      </div>

      <div className="player-right">
        <button
          className="icon-btn"
          aria-label={queueOpen ? t("player.hideQueue") : t("player.showQueue")}
          aria-pressed={queueOpen}
          onClick={onToggleQueue}
        >
          <Icon name="queue" size={1.25} />
        </button>
        <div className="player-volume">
          <Icon name="volume" size={1.25} />
          <input
            type="range"
            className="slider"
            aria-label={t("player.volume")}
            aria-valuetext={`${volume}%`}
            min={0}
            max={100}
            value={volume}
            style={fill(volume, 100)}
            onChange={(event) => setVolume(Number(event.target.value))}
          />
        </div>
      </div>
    </footer>
  );
}
