import { t } from "../i18n";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";
import { Artwork } from "./Artwork";
import { Icon } from "./Icon";
import { SeekBar, Transport, usePlayable, VolumeControl } from "./PlayerControls";
import { SleepTimerButton } from "./SleepTimer";
import { FavoriteButton } from "./TrackTable";

interface PlayerBarProps {
  queueOpen: boolean;
  onToggleQueue: () => void;
}

/** Persistent bottom bar: current track, transport and progress, queue and volume. */
export function PlayerBar({ queueOpen, onToggleQueue }: PlayerBarProps) {
  const { route, go, back, canBack } = useApp();
  const { playback, artworkUrl } = useLibrary();
  const { hint } = usePlayable();
  const { track, outputAvailable } = playback;
  const onNowPlaying = route.page === "nowPlaying";

  // The current track opens Now Playing; on that page it takes you back.
  const toggleNowPlaying = () => {
    if (!onNowPlaying) go({ page: "nowPlaying" });
    else if (canBack) back();
    else go({ page: "home" });
  };

  return (
    <footer className="player" aria-label={t("player.label")}>
      <div className="player-track">
        <button
          className="player-open"
          aria-label={onNowPlaying ? t("nowPlaying.close") : t("nowPlaying.open")}
          aria-pressed={onNowPlaying}
          title={onNowPlaying ? t("nowPlaying.close") : t("nowPlaying.open")}
          onClick={toggleNowPlaying}
        >
          <Artwork src={artworkUrl(track?.artwork, true)} size={3.5} />
          <span className="track-text">
            <span className="track-title truncate">{track ? track.title : t("player.nothingPlaying")}</span>
            <span className="track-artist truncate">
              {track ? track.artist : outputAvailable ? t("player.choose") : hint}
            </span>
          </span>
          <span className="player-open-chevron" aria-hidden="true">
            <Icon name={onNowPlaying ? "chevron-down" : "chevron-up"} size={1} />
          </span>
        </button>
        {track && <FavoriteButton track={track} alwaysVisible />}
      </div>

      <div className="player-center">
        <Transport />
        <SeekBar />
      </div>

      <div className="player-right">
        <SleepTimerButton />
        <button
          className="icon-btn"
          aria-label={queueOpen ? t("player.hideQueue") : t("player.showQueue")}
          aria-pressed={queueOpen}
          onClick={onToggleQueue}
        >
          <Icon name="queue" size={1.25} />
        </button>
        <VolumeControl />
      </div>
    </footer>
  );
}
