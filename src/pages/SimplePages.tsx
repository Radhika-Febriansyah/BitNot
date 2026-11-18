import { useEffect, useState } from "react";
import { Artwork } from "../components/Artwork";
import { CollectionGrid } from "../components/CollectionGrid";
import { EmptyState } from "../components/EmptyState";
import { Icon } from "../components/Icon";
import { TrackTable } from "../components/TrackTable";
import { t } from "../i18n";
import { api } from "../lib/api";
import { buildRows, DEFAULT_SORT, type SearchResults } from "../lib/library";
import { useApp } from "../state/AppContext";
import { useLibrary, useLibraryData } from "../state/LibraryContext";
import { useUi } from "../state/UiContext";
import { TrackListView } from "./Library";

function SimplePage({ heading, title, message }: { heading: string; title: string; message: string }) {
  return (
    <div className="page">
      <h1>{heading}</h1>
      <EmptyState title={title} message={message} />
    </div>
  );
}

export const Explore = () => (
  <SimplePage
    heading={t("nav.explore")}
    title="Online music is not available yet"
    message="BitNot only connects to online catalogs whose terms have been verified. None has been added in this build. Your local music always works offline."
  />
);

/** Favorites: newest first by default; sortable by title, artist, album, or date added. */
export function Favorites() {
  const { data: tracks } = useLibraryData(api.favorites, [], ["favorites", "tracks"]);
  if (!tracks) return <div className="page" aria-busy="true" />;
  if (tracks.length === 0) {
    return (
      <SimplePage
        heading={t("nav.favorites")}
        title="No favorites yet"
        message="Use the heart on any track to collect it here."
      />
    );
  }
  return (
    <div className="page">
      <h1>{t("nav.favorites")}</h1>
      <TrackListView tracks={tracks} label={t("nav.favorites")} />
    </div>
  );
}

const TIME_FORMAT = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
});

export function RecentlyPlayed() {
  const { notify } = useApp();
  const { artworkUrl, playTracks } = useLibrary();
  const { showDetails } = useUi();
  const { data: entries } = useLibraryData(api.history, [], ["history", "tracks"]);
  if (!entries) return <div className="page" aria-busy="true" />;
  if (entries.length === 0) {
    return (
      <SimplePage
        heading={t("nav.recent")}
        title="Nothing played yet"
        message="Your listening history will appear here, newest first."
      />
    );
  }
  const fail = () => notify("History could not be changed.", "error");
  return (
    <div className="page">
      <div className="page-title-row">
        <h1>{t("nav.recent")}</h1>
        <button className="btn" onClick={() => void api.clearHistory().catch(fail)}>
          Clear all
        </button>
      </div>
      <ul className="rows">
        {entries.map((entry) => (
          <li key={entry.id} className="row" data-unavailable={!entry.track.available}>
            <button
              className="row-open"
              onClick={() => showDetails(entry.track, { list: [entry.track], index: 0 })}
            >
              <Artwork src={artworkUrl(entry.track.artwork, true)} size={2.5} />
              <span className="track-text">
                <span className="track-title truncate">{entry.track.title}</span>
                <span className="track-artist truncate">{entry.track.artist}</span>
              </span>
            </button>
            <span className="muted row-time">{TIME_FORMAT.format(entry.playedAt * 1000)}</span>
            <button
              className="btn"
              disabled={!entry.track.available}
              onClick={() => playTracks([entry.track], 0)}
            >
              Replay
            </button>
            <button
              className="icon-btn"
              aria-label={`Remove ${entry.track.title} from history`}
              onClick={() => void api.removeHistoryEntry(entry.id).catch(fail)}
            >
              <Icon name="x" size={1.125} />
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

interface SearchProps {
  query: string;
  onQuery: (query: string) => void;
}

/**
 * Instant local search. Matching is exact on the typed text, ignoring case and
 * accents; there is no typo correction.
 */
export function Search({ query, onQuery }: SearchProps) {
  const { settings, updateSettings } = useApp();
  const { versions, overview } = useLibrary();
  const [results, setResults] = useState<SearchResults | null>(null);
  const text = query.trim();
  const tracksVersion = versions.tracks;

  useEffect(() => {
    if (!text) {
      setResults(null);
      return;
    }
    let stale = false;
    api
      .search(text)
      .then((found) => !stale && setResults(found))
      .catch(() => !stale && setResults(null));
    return () => {
      stale = true;
    };
  }, [text, tracksVersion]);

  if (!text) {
    return (
      <div className="page">
        <h1>{t("nav.search")}</h1>
        {settings.recentSearches.length > 0 ? (
          <section aria-labelledby="recent-searches">
            <div className="section-head">
              <h2 id="recent-searches">Recent searches</h2>
              <button className="btn btn-ghost" onClick={() => void updateSettings({ recentSearches: [] })}>
                Clear
              </button>
            </div>
            <div className="chip-row">
              {settings.recentSearches.map((recent) => (
                <button key={recent} className="chip" onClick={() => onQuery(recent)}>
                  {recent}
                </button>
              ))}
            </div>
          </section>
        ) : (
          <EmptyState
            title="Search your music"
            message={
              overview.trackCount > 0
                ? "Type a title, artist, or album in the search field above. Results appear as you type."
                : "Add music first, then search it by title, artist, or album."
            }
          />
        )}
      </div>
    );
  }

  const none =
    results !== null &&
    results.songs.length === 0 &&
    results.albums.length === 0 &&
    results.artists.length === 0;

  return (
    <div className="page">
      <h1>{t("nav.search")}</h1>
      {none && (
        <EmptyState
          title="No Results Found"
          message={`Nothing in your library matches “${text}”. Try another title, artist, or album.`}
        />
      )}
      {results && results.songs.length > 0 && (
        <section aria-labelledby="search-songs">
          <div className="section-head">
            <h2 id="search-songs">{t("library.songs")}</h2>
          </div>
          <TrackTable
            rows={buildRows(results.songs, "", DEFAULT_SORT).rows}
            playList={results.songs}
            label={t("library.songs")}
          />
        </section>
      )}
      {results && results.albums.length > 0 && (
        <section aria-labelledby="search-albums">
          <div className="section-head">
            <h2 id="search-albums">{t("library.albums")}</h2>
          </div>
          <CollectionGrid items={results.albums} label={t("library.albums")} />
        </section>
      )}
      {results && results.artists.length > 0 && (
        <section aria-labelledby="search-artists">
          <div className="section-head">
            <h2 id="search-artists">{t("library.artists")}</h2>
          </div>
          <CollectionGrid items={results.artists} label={t("library.artists")} />
        </section>
      )}
    </div>
  );
}
