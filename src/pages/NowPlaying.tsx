import { Artwork } from "../components/Artwork";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { SeekBar, Transport, VolumeControl } from "../components/PlayerControls";
import { t } from "../i18n";
import { formatDuration, qualityText, type Track } from "../lib/library";
import type { NowPlayingLayout } from "../lib/types";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";
import { useUi } from "../state/UiContext";

/** How many upcoming tracks the split layout lists before "and N more". */
export const NEXT_UP_SHOWN = 5;

const LAYOUTS: { value: NowPlayingLayout; label: "nowPlaying.centered" | "nowPlaying.split" }[] = [
  { value: "centered", label: "nowPlaying.centered" },
  { value: "split", label: "nowPlaying.split" },
];

/** Same ids the core gives album and artist pages (src-tauri/src/library.rs). */
export function albumPageId(track: Pick<Track, "albumArtist" | "album">): string {
  return `album:${track.albumArtist}\u001f${track.album}`;
}

export function artistPageId(track: Pick<Track, "artist">): string {
  return `artist:${track.artist}`;
}

function LayoutPicker() {
  const { settings, updateSettings } = useApp();
  return (
    <div className="chip-row" role="radiogroup" aria-label={t("nowPlaying.layout")}>
      {LAYOUTS.map((option) => (
        <button
          key={option.value}
          className="chip"
          role="radio"
          aria-checked={settings.nowPlayingLayout === option.value}
          onClick={() => void updateSettings({ nowPlayingLayout: option.value })}
        >
          {t(option.label)}
        </button>
      ))}
    </div>
  );
}

/** Title, links to the artist and album, favourite and details. */
function TrackInfo({ track }: { track: Track }) {
  const { go } = useApp();
  const { isFavorite, setFavorite } = useLibrary();
  const { showDetails } = useUi();
  const favorite = isFavorite(track.id);
  const quality = qualityText(track);

  return (
    <div className="np-info">
      <h2 className="np-title">{track.title}</h2>
      <p className="np-byline">
        <button className="link" onClick={() => go({ page: "collection", id: artistPageId(track) })}>
          {track.artist}
        </button>
        <span aria-hidden="true"> • </span>
        <button className="link" onClick={() => go({ page: "collection", id: albumPageId(track) })}>
          {track.album}
        </button>
      </p>
      {quality && <p className="np-quality">{quality}</p>}
      <div className="np-actions">
        <button
          className="icon-btn np-fav"
          data-on={favorite}
          aria-pressed={favorite}
          aria-label={t(favorite ? "table.removeFavorite" : "table.addFavorite", { title: track.title })}
          title={favorite ? t("details.inFavorites") : t("menu.addFavorite")}
          onClick={() => setFavorite(track, !favorite)}
        >
          <Icon name={favorite ? "heart-filled" : "heart"} size={1.25} />
        </button>
        <button className="btn btn-ghost" onClick={() => showDetails(track, { list: [track], index: 0 })}>
          {t("nowPlaying.details")}
        </button>
      </div>
    </div>
  );
}

function NextUp() {
  const { playback, artworkUrl } = useLibrary();
  const shown = playback.nextUp.slice(0, NEXT_UP_SHOWN);
  const more = playback.nextUpTotal - shown.length;
  return (
    <section className="np-next" aria-labelledby="np-next-title">
      <h3 id="np-next-title">{t("queue.nextUp")}</h3>
      {shown.length === 0 ? (
        <p className="muted">{t("nowPlaying.nothingNext")}</p>
      ) : (
        <ol>
          {shown.map((track, index) => (
            <li key={`${track.id}-${index}`} className="np-next-row">
              <Artwork src={artworkUrl(track.artwork, true)} size={2.5} />
              <span className="track-text">
                <span className="track-title truncate">{track.title}</span>
                <span className="track-artist truncate">{track.artist}</span>
              </span>
              <span className="num muted">{formatDuration(track.durationMs)}</span>
            </li>
          ))}
        </ol>
      )}
      {more > 0 && <p className="muted np-more">{t("queue.more", { count: more })}</p>}
    </section>
  );
}

/** Now Playing: large artwork, track details, and the same controls as the player bar. */
export function NowPlaying() {
  const { settings, go } = useApp();
  const { playback, artworkUrl } = useLibrary();
  const { track, playing } = playback;
  const layout = settings.nowPlayingLayout;

  return (
    <div className="page now-playing" data-layout={layout}>
      <div className="page-title-row">
        <h1 className="np-heading">{t("nowPlaying.title")}</h1>
        <LayoutPicker />
      </div>

      {!track ? (
        <EmptyState
          title={t("player.nothingPlaying")}
          message={t("nowPlaying.emptyText")}
          action={
            <button className="btn btn-primary" onClick={() => go({ page: "library", filter: "all" })}>
              {t("nowPlaying.browse")}
            </button>
          }
        />
      ) : (
        <div className="np-body">
          <div className="np-art-wrap" data-playing={playing}>
            {/* Keyed so a new track fades in instead of swapping abruptly. */}
            <Artwork key={track.id} className="np-art" src={artworkUrl(track.artwork)} />
          </div>
          <div className="np-side">
            <TrackInfo track={track} />
            <div className="np-controls">
              <SeekBar className="player-seek np-seek" />
              <Transport large />
              <VolumeControl className="player-volume np-volume" />
            </div>
            {layout === "split" && <NextUp />}
          </div>
        </div>
      )}
    </div>
  );
}
