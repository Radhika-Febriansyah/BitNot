import { useMemo, useState } from "react";
import { CollectionGrid } from "../components/CollectionGrid";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { LIBRARY_FILTERS } from "../components/Sidebar";
import { SortButton } from "../components/SortButton";
import { TrackTable } from "../components/TrackTable";
import { t } from "../i18n";
import { api } from "../lib/api";
import { buildRows, DEFAULT_SORT, formatSongCount, nextSort, type Sort, type Track } from "../lib/library";
import type { LibraryFilter } from "../lib/router";
import { useApp } from "../state/AppContext";
import { useLibrary, useLibraryData } from "../state/LibraryContext";
import { useUi } from "../state/UiContext";

/** A sortable track list with Play all, used for Songs and Recently Added. */
export function TrackListView({ tracks, label, initialSort = DEFAULT_SORT }: { tracks: Track[]; label: string; initialSort?: Sort }) {
  const { playTracks, playback } = useLibrary();
  const [sort, setSort] = useState<Sort>(initialSort);
  const { rows, playList } = useMemo(() => buildRows(tracks, "", sort), [tracks, sort]);
  const onSort = (field: Parameters<typeof nextSort>[1]) => setSort((current) => nextSort(current, field));
  return (
    <>
      <div className="list-actions">
        <button
          className="btn btn-primary btn-lg"
          disabled={!playback.outputAvailable}
          onClick={() => playTracks(playList, 0)}
        >
          <Icon name="play" size={1.125} />
          {t("library.playAll")}
        </button>
        <span className="muted">{formatSongCount(tracks.length)}</span>
        <span className="spacer" />
        <SortButton sort={sort} onSort={onSort} />
      </div>
      <TrackTable rows={rows} playList={playList} label={label} sort={sort} onSort={onSort} />
    </>
  );
}

function NoMusic() {
  const { addFolder } = useUi();
  return (
    <EmptyState
      title={t("library.emptyTitle")}
      message={t("library.emptyText")}
      action={
        <button className="btn btn-primary btn-lg" onClick={addFolder}>
          <Icon name="plus" size={1.125} />
          {t("library.addMusic")}
        </button>
      }
    />
  );
}

function Songs({ recent }: { recent: boolean }) {
  const { data: tracks } = useLibraryData(recent ? api.recentTracks : api.tracks, [recent], ["tracks"]);
  if (!tracks) return null;
  if (tracks.length === 0) return <NoMusic />;
  return (
    <TrackListView
      key={recent ? "recent" : "songs"}
      tracks={tracks}
      label={t(recent ? "library.recentlyAdded" : "library.songs")}
    />
  );
}

export function Library({ filter }: { filter: LibraryFilter }) {
  const { go, settings, updateSettings } = useApp();
  const { overview } = useLibrary();
  const { newPlaylist } = useUi();
  const hasMusic = overview.trackCount > 0;
  const view = settings.libraryView;
  const showsCollections = ["all", "playlists", "albums", "artists"].includes(filter);

  const body = (() => {
    switch (filter) {
      case "local":
        return <Songs recent={false} />;
      case "recent":
        return <Songs recent />;
      case "online":
        return <EmptyState title={t("library.noOnlineTitle")} message={t("library.noOnlineText")} />;
      case "playlists":
        return overview.playlists.length > 0 ? (
          <CollectionGrid items={overview.playlists} label={t("library.playlists")} view={view} />
        ) : (
          <EmptyState
            title={t("library.noPlaylistsTitle")}
            message={t("library.noPlaylistsText")}
            action={
              <button className="btn btn-primary btn-lg" onClick={() => newPlaylist()}>
                <Icon name="plus" size={1.125} />
                {t("library.newPlaylist")}
              </button>
            }
          />
        );
      case "albums":
        return hasMusic ? (
          <CollectionGrid items={overview.albums} label={t("library.albums")} view={view} />
        ) : (
          <NoMusic />
        );
      case "artists":
        return hasMusic ? (
          <CollectionGrid items={overview.artists} label={t("library.artists")} view={view} />
        ) : (
          <NoMusic />
        );
      case "all":
        if (!hasMusic && overview.playlists.length === 0) return <NoMusic />;
        return (
          <>
            {overview.playlists.length > 0 && (
              <section aria-labelledby="lib-playlists">
                <div className="section-head">
                  <h2 id="lib-playlists">{t("library.playlists")}</h2>
                </div>
                <CollectionGrid items={overview.playlists} label={t("library.playlists")} view={view} />
              </section>
            )}
            {overview.albums.length > 0 && (
              <section aria-labelledby="lib-albums">
                <div className="section-head">
                  <h2 id="lib-albums">{t("library.albums")}</h2>
                </div>
                <CollectionGrid items={overview.albums} label={t("library.albums")} view={view} />
              </section>
            )}
          </>
        );
    }
  })();

  return (
    <div className="page">
      <div className="page-title-row">
        <h1>{t("nav.library")}</h1>
        <div className="section-actions">
          {filter === "playlists" && overview.playlists.length > 0 && (
            <button className="btn" onClick={() => newPlaylist()}>
              <Icon name="plus" size={1} />
              {t("library.newPlaylist")}
            </button>
          )}
          {showsCollections && (
            <div className="view-toggle" role="radiogroup" aria-label="View">
              <button
                className="icon-btn"
                role="radio"
                aria-checked={view === "grid"}
                aria-label={t("library.viewGrid")}
                title={t("library.viewGrid")}
                onClick={() => void updateSettings({ libraryView: "grid" })}
              >
                <Icon name="grid" size={1.125} />
              </button>
              <button
                className="icon-btn"
                role="radio"
                aria-checked={view === "list"}
                aria-label={t("library.viewList")}
                title={t("library.viewList")}
                onClick={() => void updateSettings({ libraryView: "list" })}
              >
                <Icon name="list" size={1.125} />
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="chip-row" role="group" aria-label={t("library.sections")}>
        <button
          className="chip"
          aria-pressed={filter === "all"}
          onClick={() => go({ page: "library", filter: "all" })}
        >
          {t("library.all")}
        </button>
        {LIBRARY_FILTERS.map((item) => (
          <button
            key={item.filter}
            className="chip"
            aria-pressed={filter === item.filter}
            onClick={() => go({ page: "library", filter: item.filter })}
          >
            {t(item.label)}
          </button>
        ))}
        <button
          className="chip"
          aria-pressed={filter === "recent"}
          onClick={() => go({ page: "library", filter: "recent" })}
        >
          {t("library.recentlyAdded")}
        </button>
      </div>
      {body}
    </div>
  );
}
