import { useState } from "react";
import { CollectionGrid } from "../components/CollectionGrid";
import { EmptyState } from "../components/EmptyState";
import { Icon, type IconName } from "../components/Icon";
import { t } from "../i18n";
import { folderCountText } from "../lib/folders";
import type { Route } from "../lib/router";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";
import { useUi } from "../state/UiContext";

const GUIDE: { icon: IconName; title: string; text: string; to: Route }[] = [
  {
    icon: "folder",
    title: "Add Your Music",
    text: "Point BitNot at the folders where your music lives.",
    to: { page: "settings" },
  },
  {
    icon: "list-plus",
    title: "Create Playlist",
    text: "Collect tracks in your own order.",
    to: { page: "library", filter: "playlists" },
  },
  {
    icon: "compass",
    title: "Explore Music",
    text: "Discover music from legal online catalogs.",
    to: { page: "explore" },
  },
  {
    icon: "sliders",
    title: "Customize BitNot",
    text: "Theme, accent colour, and display scale.",
    to: { page: "settings" },
  },
];

const HOME_LIMIT = 12;

export function Home() {
  const { boot, settings, updateSettings, go } = useApp();
  const { overview, scan } = useLibrary();
  const { addFolder } = useUi();
  const [crashNoticeOpen, setCrashNoticeOpen] = useState(boot.previousSessionCrashed);
  const [recoveryOpen, setRecoveryOpen] = useState(boot.recoveryNotices.length > 0);
  const hasMusic = overview.trackCount > 0;

  return (
    <div className="page">
      <h1>Home</h1>

      {crashNoticeOpen && (
        <div className="notice" role="status">
          <Icon name="alert" size={1.125} />
          <p>
            BitNot did not close properly last time. Your data passed its integrity check and
            loaded normally.
          </p>
          <button className="btn" onClick={() => go({ page: "settings" })}>
            View diagnostics
          </button>
          <button
            className="icon-btn"
            aria-label="Dismiss notice"
            onClick={() => setCrashNoticeOpen(false)}
          >
            <Icon name="x" size={1.125} />
          </button>
        </div>
      )}

      {recoveryOpen &&
        boot.recoveryNotices.map((notice) => (
          <div key={notice} className="notice" role="alert">
            <Icon name="alert" size={1.125} />
            <p className="selectable">{notice}</p>
            <button
              className="icon-btn"
              aria-label={t("recovery.dismiss")}
              onClick={() => setRecoveryOpen(false)}
            >
              <Icon name="x" size={1.125} />
            </button>
          </div>
        ))}

      {!settings.guideDismissed && (
        <section aria-labelledby="guide-title">
          <div className="section-head">
            <h2 id="guide-title">Get started</h2>
            <button
              className="btn btn-ghost"
              onClick={() => void updateSettings({ guideDismissed: true })}
            >
              Dismiss
            </button>
          </div>
          <ul className="rows">
            {GUIDE.map((item) => (
              <li key={item.title}>
                <button className="row row-link" onClick={() => go(item.to)}>
                  <span className="art accent row-icon">
                    <Icon name={item.icon} size={1.125} />
                  </span>
                  <span className="track-text">
                    <span className="track-title">{item.title}</span>
                    <span className="track-artist">{item.text}</span>
                  </span>
                  <Icon name="chevron-right" size={1.125} />
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      {overview.playlists.length > 0 && (
        <section aria-labelledby="home-playlists-title">
          <div className="section-head">
            <h2 id="home-playlists-title">Your playlists</h2>
            <button
              className="btn btn-ghost"
              onClick={() => go({ page: "library", filter: "playlists" })}
            >
              Show all
            </button>
          </div>
          <CollectionGrid items={overview.playlists.slice(0, HOME_LIMIT)} label="Your playlists" />
        </section>
      )}

      <section aria-labelledby="home-albums-title">
        <div className="section-head">
          <h2 id="home-albums-title">{hasMusic ? "Albums" : "Your music"}</h2>
          {overview.albums.length > HOME_LIMIT && (
            <button className="btn btn-ghost" onClick={() => go({ page: "library", filter: "albums" })}>
              Show all
            </button>
          )}
        </div>
        {hasMusic ? (
          <CollectionGrid items={overview.albums.slice(0, HOME_LIMIT)} label="Albums" />
        ) : (
          <EmptyState
            title={scan.running ? "Scanning your music…" : "Your library is empty"}
            message={
              scan.running
                ? scan.total > 0
                  ? `${scan.done} of ${scan.total} files checked.`
                  : "Looking for audio files."
                : settings.musicFolders.length > 0
                  ? `${folderCountText(settings.musicFolders.length)} No playable audio files were found in them. Subfolders are only scanned when that option is turned on in Settings.`
                  : "Add a music folder to begin."
            }
            action={
              scan.running ? undefined : (
                <button className="btn btn-primary btn-lg" onClick={addFolder}>
                  <Icon name="plus" size={1.125} />
                  Add Music
                </button>
              )
            }
          />
        )}
      </section>
    </div>
  );
}
