import { useEffect, useRef, useState } from "react";
import { Collection } from "../pages/Collection";
import { Home } from "../pages/Home";
import { Library } from "../pages/Library";
import { NowPlaying } from "../pages/NowPlaying";
import { Settings } from "../pages/Settings";
import { Explore, Favorites, RecentlyPlayed, Search } from "../pages/SimplePages";
import type { Route } from "../lib/router";
import { NARROW_QUERY, useMediaQuery } from "../lib/useMediaQuery";
import { useApp } from "../state/AppContext";
import { useLibrary } from "../state/LibraryContext";
import { UiProvider } from "../state/UiContext";
import { SEEK_STEP_MS, VOLUME_STEP } from "../lib/shortcuts";
import { useShortcuts } from "../lib/useShortcuts";
import { ShortcutsDialog } from "./ShortcutsDialog";
import { PlayerBar } from "./PlayerBar";
import { QueuePanel } from "./QueuePanel";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";

const MAX_RECENT_SEARCHES = 5;
/** Below this width the queue floats over the content (matches shell.css). */
const QUEUE_OVERLAY_QUERY = "(max-width: 1240px)";

function routeKey(route: Route): string {
  if (route.page === "collection") return `collection:${route.id}`;
  return route.page;
}

/** The four regions of the reference: sidebar, centre column, queue, player bar. */
export function Shell() {
  const { route, settings, updateSettings, notify } = useApp();
  const library = useLibrary();
  const { playback } = library;
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const canPlay = playback.outputAvailable && (playback.track !== null || playback.nextUp.length > 0);

  useShortcuts({
    togglePlay: () => canPlay && library.toggle(),
    next: () => canPlay && library.next(),
    previous: () => canPlay && library.previous(),
    seekForward: () => library.seekBy(SEEK_STEP_MS),
    seekBack: () => library.seekBy(-SEEK_STEP_MS),
    volumeUp: () => library.setVolume(library.volume + VOLUME_STEP),
    volumeDown: () => library.setVolume(library.volume - VOLUME_STEP),
    mute: library.toggleMute,
    search: () => {
      const input = document.querySelector<HTMLInputElement>(".top-search input");
      input?.focus();
      input?.select();
    },
    help: () => setShortcutsOpen(true),
  });
  const narrow = useMediaQuery(NARROW_QUERY);
  const queueOverlay = useMediaQuery(QUEUE_OVERLAY_QUERY);
  const rail = narrow || settings.sidebarCollapsed;
  const queueOpen = settings.queuePanelOpen;
  const contentRef = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState("");
  const key = routeKey(route);

  useEffect(() => {
    contentRef.current?.scrollTo({ top: 0 });
  }, [key]);

  // Tell the user when a track had to be skipped.
  const playbackError = playback.error;
  useEffect(() => {
    if (playbackError) notify(playbackError, "error");
  }, [playbackError, notify]);

  const toggleQueue = () => void updateSettings({ queuePanelOpen: !queueOpen });

  const rememberSearch = () => {
    const text = query.trim();
    if (!text) return;
    const rest = settings.recentSearches.filter((s) => s.toLowerCase() !== text.toLowerCase());
    void updateSettings({ recentSearches: [text, ...rest].slice(0, MAX_RECENT_SEARCHES) });
  };

  const page = (() => {
    switch (route.page) {
      case "home":
        return <Home />;
      case "explore":
        return <Explore />;
      case "search":
        return <Search query={query} onQuery={setQuery} />;
      case "library":
        return <Library filter={route.filter} />;
      case "favorites":
        return <Favorites />;
      case "recent":
        return <RecentlyPlayed />;
      case "collection":
        return <Collection key={route.id} id={route.id} />;
      case "nowPlaying":
        return <NowPlaying />;
      case "settings":
        return <Settings />;
    }
  })();

  return (
    <UiProvider>
      <div className="shell" data-rail={rail} data-queue={queueOpen}>
        <Sidebar rail={rail} />
        <div className="main">
          <TopBar query={query} onQuery={setQuery} onSubmit={rememberSearch} />
          <main className="content scroll" ref={contentRef} tabIndex={-1}>
            <div className="page-fade" key={key}>
              {page}
            </div>
          </main>
        </div>
        {queueOpen && <QueuePanel onClose={toggleQueue} overlay={queueOverlay} />}
        <PlayerBar queueOpen={queueOpen} onToggleQueue={toggleQueue} />
      </div>
      <ShortcutsDialog open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
    </UiProvider>
  );
}
