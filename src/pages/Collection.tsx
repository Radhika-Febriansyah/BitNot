import { useMemo, useState } from "react";
import { Artwork } from "../components/Artwork";
import { CollectionGrid } from "../components/CollectionGrid";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { SortButton } from "../components/SortButton";
import { TrackTable } from "../components/TrackTable";
import { t } from "../i18n";
import { api } from "../lib/api";
import {
  buildRows,
  DEFAULT_SORT,
  formatSongCount,
  formatTotalDuration,
  kindLabel,
  moved,
  nextSort,
  type Sort,
  type Track,
} from "../lib/library";
import { useLibrary, useLibraryData } from "../state/LibraryContext";
import { useUi, type MenuItem } from "../state/UiContext";

function shuffled(tracks: Track[]): Track[] {
  const copy = [...tracks];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

/** Playlist, album, or artist page: the layout the design reference shows. */
export function Collection({ id }: { id: string }) {
  const library = useLibrary();
  const { playback, artworkUrl, playTracks, toggle, addToQueue } = library;
  const { openMenu, editPlaylist, confirmDeletePlaylist } = useUi();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>(DEFAULT_SORT);
  // A dragged row is shown in its new place at once, before the core confirms.
  const [pending, setPending] = useState<Track[] | null>(null);
  const { data: loaded, error } = useLibraryData(
    () =>
      api.collection(id).then((detail) => {
        setPending(null);
        return detail;
      }),
    [id],
    // Artist pages show Popular Tracks, which follow listening history.
    id.startsWith("artist:") ? ["tracks", "history"] : ["tracks", "playlists"],
  );
  const tracks = pending ?? loaded?.tracks ?? [];
  const { rows, playList } = useMemo(() => buildRows(tracks, query, sort), [tracks, query, sort]);

  if (error) {
    return (
      <div className="page">
        <EmptyState title="This item is not available" message={error} />
      </div>
    );
  }
  if (!loaded) return <div className="page" aria-busy="true" />;
  const collection = loaded;

  const playable = tracks.filter((track) => track.available);
  const totalMs = tracks.reduce((sum, track) => sum + track.durationMs, 0);
  const currentIsHere = playback.track !== null && tracks.some((x) => x.id === playback.track?.id);
  const playingHere = playback.playing && currentIsHere;
  const isPlaylist = collection.kind === "playlist";
  const isArtist = collection.kind === "artist";
  // Entries can be dragged only while the stored order is what is shown.
  const reorderable = isPlaylist && sort.field === "default" && !sort.descending && !query.trim();
  const move = (from: number, to: number) => {
    setPending(moved(tracks, from, to));
    library.movePlaylistTrack(collection.id, from, to);
  };

  const more: MenuItem[] = [
    {
      label: t("menu.addAllToQueue"),
      disabled: playable.length === 0,
      onSelect: () => addToQueue(playable),
    },
  ];
  if (isPlaylist) {
    more.push(
      { separator: true },
      { label: t("menu.editPlaylist"), onSelect: () => editPlaylist(collection) },
      { label: t("menu.chooseCover"), onSelect: () => library.choosePlaylistCover(collection.id) },
    );
    if (collection.customCover) {
      more.push({ label: t("menu.removeCover"), onSelect: () => library.clearPlaylistCover(collection.id) });
    }
    more.push({
      label: t("menu.deletePlaylist"),
      danger: true,
      onSelect: () => confirmDeletePlaylist(collection.id, collection.title),
    });
  }
  if (collection.kind === "album") {
    more.push(
      { separator: true },
      { label: t("menu.changeAlbumCover"), onSelect: () => library.chooseArtwork(tracks) },
      { label: t("menu.resetAlbumCover"), onSelect: () => library.clearArtwork(tracks) },
    );
  }

  const table = (
    <TrackTable
      rows={rows}
      playList={playList}
      label={collection.title}
      playlistId={isPlaylist ? collection.id : undefined}
      entryCount={tracks.length}
      onMove={reorderable ? move : undefined}
      sort={sort}
      onSort={(field) => setSort((current) => nextSort(current, field))}
    />
  );

  return (
    <div className="page collection">
      <header className="hero">
        <Artwork
          src={artworkUrl(collection.artwork)}
          size={13.25}
          className={isArtist ? "hero-art round" : "hero-art"}
          fallback={isArtist ? "user" : "music"}
        />
        <div className="hero-text">
          <span className="hero-kind">{kindLabel(collection.kind)}</span>
          <h1 className="hero-title">{collection.title}</h1>
          {collection.description && <p className="hero-description">{collection.description}</p>}
          <p className="hero-meta">
            {collection.kind === "album" && (
              <>
                {collection.owner} <span aria-hidden="true">•</span>{" "}
              </>
            )}
            {formatSongCount(tracks.length)}
            {tracks.length > 0 && `, ${formatTotalDuration(totalMs)}`}
          </p>
        </div>
      </header>

      <div className="actions">
        <button
          className="play-fab"
          aria-label={playingHere ? t("player.pause") : t("player.play")}
          disabled={playable.length === 0 || !playback.outputAvailable}
          onClick={() => (currentIsHere ? toggle() : playTracks(playList, 0))}
        >
          <Icon name={playingHere ? "pause" : "play"} size={1.75} />
        </button>
        <button
          className="icon-btn action"
          aria-label={t("player.shufflePlay")}
          title={t("player.shufflePlay")}
          disabled={playable.length === 0 || !playback.outputAvailable}
          onClick={() => playTracks(shuffled(playable), 0)}
        >
          <Icon name="shuffle" size={1.625} />
        </button>
        <button
          className="icon-btn action"
          aria-label={t("player.moreActions")}
          aria-haspopup="menu"
          onClick={(event) => {
            const box = event.currentTarget.getBoundingClientRect();
            openMenu(more, box.left, box.bottom + 4);
          }}
        >
          <Icon name="dots" size={1.625} />
        </button>
        <div className="spacer" />
        <SortButton sort={sort} onSort={(field) => setSort((current) => nextSort(current, field))} />
        <label className="field track-filter">
          <Icon name="search" size={1.125} />
          <input
            type="search"
            placeholder={t("table.filter")}
            aria-label={t("table.filterIn", { name: collection.title })}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
      </div>

      {isArtist && collection.popular.length > 0 && !query.trim() && (
        <section aria-labelledby="artist-popular">
          <div className="section-head">
            <h2 id="artist-popular">{t("artist.popular")}</h2>
          </div>
          <TrackTable
            rows={buildRows(collection.popular, "", DEFAULT_SORT).rows}
            playList={collection.popular}
            label={t("artist.popular")}
          />
        </section>
      )}
      {isArtist && collection.albums.length > 0 && !query.trim() && (
        <section aria-labelledby="artist-albums">
          <div className="section-head">
            <h2 id="artist-albums">{t("artist.discography")}</h2>
          </div>
          <CollectionGrid items={collection.albums} label={t("artist.discography")} showYear />
        </section>
      )}

      {isArtist ? (
        <section aria-labelledby="artist-tracks">
          <div className="section-head">
            <h2 id="artist-tracks">{t("artist.allTracks")}</h2>
          </div>
          {table}
        </section>
      ) : (
        tracks.length > 0 && table
      )}
      {rows.length === 0 && (
        <p className="no-results" role="status">
          {query.trim() ? t("table.noResults", { query: query.trim() }) : t("table.emptyPlaylist")}
        </p>
      )}
    </div>
  );
}
