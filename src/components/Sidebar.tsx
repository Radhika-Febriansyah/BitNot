import { t, type MessageKey } from "../i18n";
import { formatSongCount } from "../lib/library";
import { sameRoute, type LibraryFilter, type Route } from "../lib/router";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";
import { useUi } from "../state/UiContext";
import { Artwork } from "./Artwork";
import { Icon, type IconName } from "./Icon";

export const LIBRARY_FILTERS: { filter: LibraryFilter; label: MessageKey }[] = [
  { filter: "playlists", label: "library.playlists" },
  { filter: "albums", label: "library.albums" },
  { filter: "artists", label: "library.artists" },
  { filter: "local", label: "library.songs" },
  { filter: "online", label: "library.online" },
];

function NavItem({ icon, label, to, rail }: { icon: IconName; label: string; to: Route; rail: boolean }) {
  const { route, go } = useApp();
  return (
    <button
      className="nav-item"
      aria-current={sameRoute(route, to) ? "page" : undefined}
      aria-label={rail ? label : undefined}
      title={rail ? label : undefined}
      onClick={() => go(to)}
    >
      <Icon name={icon} size={1.375} />
      {!rail && <span>{label}</span>}
    </button>
  );
}

interface ItemProps {
  title: string;
  subtitle: string;
  to: Route;
  rail: boolean;
  artworkUrl?: string;
  icon?: IconName;
}

/** One row of the library list: square thumbnail, title, and a second line. */
function LibraryItem({ title, subtitle, to, rail, artworkUrl, icon }: ItemProps) {
  const { route, go } = useApp();
  return (
    <li>
      <button
        className="lib-item"
        aria-current={sameRoute(route, to) ? "page" : undefined}
        aria-label={rail ? title : undefined}
        title={rail ? title : undefined}
        onClick={() => go(to)}
      >
        <Artwork src={artworkUrl} size={2.75} fallback={icon} accent={Boolean(icon)} />
        {!rail && (
          <span className="track-text">
            <span className="track-title truncate">{title}</span>
            <span className="track-artist truncate">{subtitle}</span>
          </span>
        )}
      </button>
    </li>
  );
}

export function Sidebar({ rail }: { rail: boolean }) {
  const { route, go } = useApp();
  const { overview, artworkUrl, favoriteCount } = useLibrary();
  const { openMenu, newPlaylist, addFolder } = useUi();
  const filter = route.page === "library" ? route.filter : null;
  // Playlists first, then albums, like a personal collection list.
  const collections = [...overview.playlists, ...overview.albums];
  const collectionLabel = t("library.collection");

  return (
    <nav className={rail ? "sidebar rail" : "sidebar"} aria-label={t("nav.main")}>
      <div className="side-nav">
        <NavItem icon="home" label={t("nav.home")} to={{ page: "home" }} rail={rail} />
        <NavItem icon="compass" label={t("nav.explore")} to={{ page: "explore" }} rail={rail} />
        <NavItem icon="search" label={t("nav.search")} to={{ page: "search" }} rail={rail} />
      </div>

      <div className="side-divider" />

      <div className="lib-head">
        <button
          className="lib-title"
          aria-current={route.page === "library" ? "page" : undefined}
          aria-label={rail ? t("nav.library") : undefined}
          title={rail ? t("nav.library") : undefined}
          onClick={() => go({ page: "library", filter: "all" })}
        >
          <Icon name="library" size={1.375} />
          {!rail && <span>{t("nav.library")}</span>}
        </button>
        {!rail && (
          <>
            <button
              className="icon-btn"
              aria-label={t("library.searchYourMusic")}
              title={t("library.searchYourMusic")}
              onClick={() => go({ page: "search" })}
            >
              <Icon name="search" size={1.125} />
            </button>
            <button
              className="icon-btn"
              aria-label={t("library.add")}
              aria-haspopup="menu"
              title={t("library.add")}
              onClick={(event) => {
                const box = event.currentTarget.getBoundingClientRect();
                openMenu(
                  [
                    { label: t("library.newPlaylist"), onSelect: () => newPlaylist() },
                    { label: t("library.addFolder"), onSelect: addFolder },
                  ],
                  box.left,
                  box.bottom + 4,
                );
              }}
            >
              <Icon name="plus" size={1.25} />
            </button>
          </>
        )}
      </div>

      {!rail && (
        <div className="chip-row side-chips" role="group" aria-label={t("library.sections")}>
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
        </div>
      )}

      <ul className="lib-list scroll">
        <LibraryItem
          title={t("nav.favorites")}
          subtitle={
            favoriteCount > 0 ? `${collectionLabel} • ${formatSongCount(favoriteCount)}` : collectionLabel
          }
          icon="heart-filled"
          to={{ page: "favorites" }}
          rail={rail}
        />
        <LibraryItem
          title={t("nav.recent")}
          subtitle={collectionLabel}
          icon="clock"
          to={{ page: "recent" }}
          rail={rail}
        />
        {collections.map((collection) => (
          <LibraryItem
            key={collection.id}
            title={collection.title}
            subtitle={collection.subtitle}
            artworkUrl={artworkUrl(collection.artwork, true)}
            to={{ page: "collection", id: collection.id }}
            rail={rail}
          />
        ))}
        {!rail && collections.length === 0 && <li className="lib-hint">{t("library.listHint")}</li>}
      </ul>
    </nav>
  );
}
