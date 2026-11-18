import { formatSongCount, type Collection } from "../lib/library";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";
import { Artwork } from "./Artwork";

interface CollectionGridProps {
  items: Collection[];
  label: string;
  /** "grid" (artwork first) or "list" (compact rows). */
  view?: "grid" | "list";
  /** Show the release year as the second line (artist discography). */
  showYear?: boolean;
}

/** Playlists, albums, and artists as an artwork grid or as a compact list. */
export function CollectionGrid({ items, label, view = "grid", showYear }: CollectionGridProps) {
  const { go } = useApp();
  const { artworkUrl } = useLibrary();
  const second = (item: Collection) =>
    showYear ? (item.year ? String(item.year) : "") : item.subtitle;

  if (view === "list") {
    return (
      <ul className="rows" aria-label={label}>
        {items.map((item) => (
          <li key={item.id}>
            <button className="row row-link" onClick={() => go({ page: "collection", id: item.id })}>
              <Artwork
                src={artworkUrl(item.artwork, true)}
                size={2.75}
                className={item.kind === "artist" ? "round" : undefined}
                fallback={item.kind === "artist" ? "user" : "music"}
              />
              <span className="track-text">
                <span className="track-title truncate">{item.title}</span>
                <span className="track-artist truncate">{second(item)}</span>
              </span>
              <span className="muted row-time">{formatSongCount(item.trackCount)}</span>
            </button>
          </li>
        ))}
      </ul>
    );
  }

  return (
    <ul className="grid" aria-label={label}>
      {items.map((item) => (
        <li key={item.id}>
          <button className="grid-item" onClick={() => go({ page: "collection", id: item.id })}>
            <Artwork
              src={artworkUrl(item.artwork, true)}
              className={item.kind === "artist" ? "grid-art round" : "grid-art"}
              fallback={item.kind === "artist" ? "user" : "music"}
            />
            <span className="track-title truncate">{item.title}</span>
            <span className="track-artist truncate">{second(item)}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}
