import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t, tn } from "../i18n";
import { api, errorMessage, EVENTS } from "../lib/api";
import {
  EMPTY_OVERVIEW,
  EMPTY_PLAYBACK,
  scanChangedLibrary,
  scanSummaryText,
  type Overview,
  type PlaybackState,
  type Progress,
  type Repeat,
  type ScanSummary,
  type Scope,
  type SleepRequest,
  type Track,
  type TrackEdit,
} from "../lib/library";
import { PlaybackClock, type ClockReading } from "../lib/playbackClock";
import { useApp } from "./AppContext";

interface ScanStatus {
  running: boolean;
  done: number;
  total: number;
  summary: ScanSummary | null;
}

type Versions = Record<Scope, number>;
const NO_VERSIONS: Versions = { tracks: 0, playlists: 0, favorites: 0, history: 0, trash: 0 };
/** Volume is saved to settings this long after the slider stops moving. */
const VOLUME_SAVE_DELAY_MS = 400;

interface LibraryContextValue {
  overview: Overview;
  playback: PlaybackState;
  /** One counter per part of the library; a view reloads only on the parts it shows. */
  versions: Versions;
  scan: ScanStatus;
  /** Live volume (0–100), updated immediately while the slider moves. */
  volume: number;
  /** True while sound is muted; `volume` keeps the level to return to. */
  muted: boolean;
  toggleMute: () => void;
  /** Moves the playback position by `deltaMs` (negative goes back). */
  seekBy: (deltaMs: number) => void;
  /** URL for a cached cover. `thumb` asks for the small static version. */
  artworkUrl: (name: string | null | undefined, thumb?: boolean) => string | undefined;
  startScan: () => Promise<void>;
  isFavorite: (trackId: number) => boolean;
  favoriteCount: number;

  playTracks: (tracks: Track[], start: number) => void;
  toggle: () => void;
  next: () => void;
  previous: () => void;
  seek: (positionMs: number) => void;
  setVolume: (percent: number) => void;
  setShuffle: (on: boolean) => void;
  setRepeat: (repeat: Repeat) => void;
  playNext: (track: Track) => void;
  addToQueue: (tracks: Track[]) => void;
  removeFromQueue: (offset: number) => void;
  moveInQueue: (from: number, to: number) => void;
  clearQueue: () => void;
  playQueued: (offset: number) => void;
  /** Arms the sleep timer (replacing any set before). */
  setSleep: (request: SleepRequest) => void;
  cancelSleep: () => void;
  saveQueueAsPlaylist: (name: string) => Promise<string | null>;

  setFavorite: (track: Track, on: boolean) => void;
  createPlaylist: (name: string, tracks?: Track[]) => Promise<string | null>;
  updatePlaylist: (id: string, name: string, description: string) => Promise<boolean>;
  choosePlaylistCover: (id: string) => void;
  clearPlaylistCover: (id: string) => void;
  deletePlaylist: (id: string) => Promise<boolean>;
  addToPlaylist: (id: string, tracks: Track[], playlistName: string) => void;
  removeFromPlaylist: (id: string, index: number) => void;
  movePlaylistTrack: (id: string, from: number, to: number) => void;

  removeFromLibrary: (tracks: Track[]) => void;
  editTrack: (id: number, edit: TrackEdit) => Promise<Track | null>;
  chooseArtwork: (tracks: Track[]) => void;
  clearArtwork: (tracks: Track[]) => void;
  preferSource: (track: Track) => void;
}

const LibraryContext = createContext<LibraryContextValue | null>(null);
/**
 * The playback clock is its own context and is never replaced, so position
 * changes reach only the components that subscribe to it (the seek bars).
 */
const ClockContext = createContext<PlaybackClock | null>(null);

function readingFromState(state: PlaybackState): ClockReading {
  return {
    trackId: state.track?.id ?? null,
    positionMs: state.positionMs,
    playing: state.playing,
    clockMs: state.clockMs,
    durationMs: state.track?.durationMs ?? null,
  };
}

/** `abc.jpg` or `abc.anim.gif` → `abc.thumb.jpg` (mirrors artwork.rs). */
export function thumbName(name: string): string {
  return `${name.split(".")[0]}.thumb.jpg`;
}

export function LibraryProvider({ children }: { children: ReactNode }) {
  const { boot, notify } = useApp();
  const [overview, setOverview] = useState<Overview>(EMPTY_OVERVIEW);
  const [favorites, setFavorites] = useState<ReadonlySet<number>>(new Set());
  const [playback, setPlayback] = useState<PlaybackState>(EMPTY_PLAYBACK);
  const [versions, setVersions] = useState<Versions>(NO_VERSIONS);
  const [volume, setVolumeState] = useState(boot.settings.volume);
  const [muted, setMuted] = useState(false);
  const [clock] = useState(() => new PlaybackClock());
  const [scan, setScan] = useState<ScanStatus>({ running: false, done: 0, total: 0, summary: null });
  const playbackRef = useRef(playback);
  playbackRef.current = playback;
  const scanningRef = useRef(false);
  const volumeTimer = useRef<number | undefined>(undefined);

  const fail = useCallback((error: unknown) => notify(errorMessage(error), "error"), [notify]);

  const applyPlayback = useCallback(
    (state: PlaybackState) => {
      setPlayback(state);
      clock.update(readingFromState(state));
    },
    [clock],
  );

  useEffect(() => clock.attach(), [clock]);

  const refreshPlayback = useCallback(() => {
    api.playbackState().then(applyPlayback).catch(fail);
  }, [applyPlayback, fail]);

  const refreshOverview = useCallback(() => {
    api
      .overview()
      .then((next) => {
        setOverview(next);
        setFavorites(new Set(next.favoriteIds));
      })
      .catch(fail);
  }, [fail]);

  const bump = useCallback((scope: Scope) => {
    setVersions((current) => ({ ...current, [scope]: current[scope] + 1 }));
  }, []);

  const reportScan = useCallback(
    (summary: ScanSummary) => {
      setScan({ running: false, done: 0, total: 0, summary });
      if (scanChangedLibrary(summary)) notify(scanSummaryText(summary));
      if (summary.unavailableFolders.length > 0) {
        notify(tn("scan.unreachable", summary.unavailableFolders.length), "error");
      }
    },
    [notify],
  );

  // Core events.
  useEffect(() => {
    const stops = [
      listen<Progress>(EVENTS.progress, ({ payload }) => {
        const current = playbackRef.current;
        const known = current.track?.id === payload.trackId ? current.track : null;
        clock.update({ ...payload, durationMs: known?.durationMs ?? null });
        if (payload.playing !== current.playing || payload.trackId !== (current.track?.id ?? null)) {
          refreshPlayback();
        }
      }),
      listen(EVENTS.playbackChanged, refreshPlayback),
      listen<Scope>(EVENTS.libraryChanged, ({ payload }) => {
        bump(payload);
        // The overview holds albums, artists, playlists, and favorite ids.
        // History and bin changes do not alter it, and a favorite toggle is
        // applied locally at once, so those do not reload it.
        if (payload === "tracks" || payload === "playlists") refreshOverview();
      }),
      listen<{ done: number; total: number }>(EVENTS.scanProgress, ({ payload }) =>
        setScan((current) => ({ ...current, running: true, done: payload.done, total: payload.total })),
      ),
      // A scan started by the folder watcher rather than by this window.
      listen<ScanSummary>(EVENTS.scanFinished, ({ payload }) => reportScan(payload)),
      // The core paused playback for the sleep timer; the state follows in
      // playback:changed, so only the message is needed here.
      listen(EVENTS.sleepEnded, () => notify(t("sleep.ended"))),
    ];
    return () => stops.forEach((stop) => void stop.then((off) => off()));
  }, [bump, clock, notify, refreshOverview, refreshPlayback, reportScan]);

  const startScan = useCallback(async () => {
    if (scanningRef.current) return;
    scanningRef.current = true;
    setScan((current) => ({ ...current, running: true, done: 0, total: 0 }));
    try {
      reportScan(await api.scan());
    } catch (error) {
      setScan((current) => ({ ...current, running: false }));
      fail(error);
    } finally {
      scanningRef.current = false;
    }
  }, [fail, reportScan]);

  // Load on start and on profile change; scan on startup when folders are set.
  const hasFolders = boot.settings.musicFolders.length > 0;
  const ready = boot.setupComplete;
  const profileVolume = boot.settings.volume;
  useEffect(() => {
    refreshOverview();
    refreshPlayback();
    setVolumeState(profileVolume);
    setMuted(false);
    setVersions(NO_VERSIONS);
    if (ready && hasFolders) void startScan();
    // Folder edits trigger their own scan; this runs per profile and after setup.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [boot.activeProfileId, ready]);

  const act = useCallback(
    (operation: Promise<PlaybackState>) => {
      operation.then(applyPlayback).catch(fail);
    },
    [applyPlayback, fail],
  );

  const value = useMemo<LibraryContextValue>(() => {
    const artworkDir = boot.artworkDir;
    const ids = (tracks: Track[]) => tracks.map((track) => track.id);
    const pickAndSet = (apply: (image: string) => Promise<void>) => {
      api
        .pickImage()
        .then((image) => (image ? apply(image) : undefined))
        .catch(fail);
    };
    return {
      overview,
      playback,
      versions,
      scan,
      volume,
      muted,
      toggleMute: () => {
        // Mute is not saved: the stored volume stays the level to return to.
        const next = !muted;
        setMuted(next);
        api.setVolumeLive(next ? 0 : volume).catch(fail);
      },
      seekBy: (deltaMs) => {
        const track = playback.track;
        if (!track) return;
        const target = Math.min(Math.max(0, clock.position() + deltaMs), Math.max(0, track.durationMs - 1000));
        act(api.seek(Math.floor(target)));
      },
      artworkUrl: (name, thumb) =>
        name ? convertFileSrc(`${artworkDir}\\${thumb ? thumbName(name) : name}`) : undefined,
      startScan,
      isFavorite: (trackId) => favorites.has(trackId),
      favoriteCount: favorites.size,

      playTracks: (tracks, start) => {
        // Unavailable tracks are left out; keep the start on the chosen track.
        const target = tracks[start];
        const playable = tracks.filter((track) => track.available);
        if (playable.length === 0) {
          notify(t("common.notPlayable"), "error");
          return;
        }
        const index = target ? Math.max(0, playable.indexOf(target)) : 0;
        act(api.playTracks(ids(playable), index));
      },
      toggle: () => act(api.toggle()),
      next: () => act(api.next()),
      previous: () => act(api.previous()),
      seek: (ms) => act(api.seek(Math.max(0, Math.floor(ms)))),
      setVolume: (percent) => {
        const next = Math.min(100, Math.max(0, Math.round(percent)));
        setVolumeState(next);
        setMuted(false); // changing the volume always makes it audible again
        // The engine changes at once; settings are written when the slider rests.
        api.setVolumeLive(next).catch(fail);
        window.clearTimeout(volumeTimer.current);
        volumeTimer.current = window.setTimeout(() => {
          api.updateSettings({ volume: next }).catch(fail);
        }, VOLUME_SAVE_DELAY_MS);
      },
      setShuffle: (on) => act(api.setShuffle(on)),
      setRepeat: (repeat) => act(api.setRepeat(repeat)),
      playNext: (track) => {
        act(api.queuePlayNext(track.id));
        notify(t("queue.playNext", { title: track.title }));
      },
      addToQueue: (tracks) => {
        const playable = tracks.filter((track) => track.available);
        if (playable.length === 0) {
          notify(t("common.notPlayable"), "error");
          return;
        }
        // One request and one notification, however many tracks.
        act(api.queueAddMany(ids(playable)));
        notify(
          playable.length === 1
            ? t("queue.addedOne", { title: playable[0].title })
            : tn("queue.addedMany", playable.length),
        );
      },
      removeFromQueue: (offset) => act(api.queueRemove(offset)),
      moveInQueue: (from, to) => act(api.queueMove(from, to)),
      clearQueue: () => act(api.queueClear()),
      playQueued: (offset) => act(api.queuePlay(offset)),
      setSleep: (request) => act(api.sleepSet(request)),
      cancelSleep: () => act(api.sleepCancel()),
      saveQueueAsPlaylist: (name) =>
        api.playlistFromQueue(name).catch((error) => {
          fail(error);
          return null;
        }),

      setFavorite: (track, on) => {
        // Shown at once; put back if the core refuses.
        setFavorites((current) => {
          const next = new Set(current);
          if (on) next.add(track.id);
          else next.delete(track.id);
          return next;
        });
        api.setFavorite(track.id, on).catch((error) => {
          fail(error);
          refreshOverview();
        });
      },
      createPlaylist: async (name, tracks) => {
        try {
          const id = await api.createPlaylist(name);
          if (tracks && tracks.length > 0) await api.addToPlaylist(id, ids(tracks));
          return id;
        } catch (error) {
          fail(error);
          return null;
        }
      },
      updatePlaylist: (id, name, description) =>
        api
          .updatePlaylist(id, name, description)
          .then(() => true)
          .catch((error) => {
            fail(error);
            return false;
          }),
      choosePlaylistCover: (id) => pickAndSet((image) => api.setPlaylistCover(id, image)),
      clearPlaylistCover: (id) => {
        api.setPlaylistCover(id, null).catch(fail);
      },
      deletePlaylist: (id) =>
        api
          .deletePlaylist(id)
          .then(() => true)
          .catch((error) => {
            fail(error);
            return false;
          }),
      addToPlaylist: (id, tracks, playlistName) => {
        api
          .addToPlaylist(id, ids(tracks))
          .then(() => notify(t("playlist.added", { name: playlistName })))
          .catch(fail);
      },
      removeFromPlaylist: (id, index) => {
        api.removeFromPlaylist(id, index).catch(fail);
      },
      movePlaylistTrack: (id, from, to) => {
        api.movePlaylistTrack(id, from, to).catch(fail);
      },

      removeFromLibrary: (tracks) => {
        api
          .removeTracks(ids(tracks))
          .then((count) => count > 0 && notify(tn("remove.done", count)))
          .catch(fail);
      },
      editTrack: (id, edit) =>
        api.editTrack(id, edit).catch((error) => {
          fail(error);
          return null;
        }),
      chooseArtwork: (tracks) => pickAndSet((image) => api.setTrackArtwork(ids(tracks), image)),
      clearArtwork: (tracks) => {
        api.setTrackArtwork(ids(tracks), null).catch(fail);
      },
      preferSource: (track) => {
        api.preferSource(track.id).catch(fail);
      },
    };
  }, [act, boot.artworkDir, fail, favorites, notify, overview, playback, refreshOverview, scan, startScan, versions, volume, muted, clock]);

  return (
    <LibraryContext.Provider value={value}>
      <ClockContext.Provider value={clock}>{children}</ClockContext.Provider>
    </LibraryContext.Provider>
  );
}

export function useLibrary(): LibraryContextValue {
  const value = useContext(LibraryContext);
  if (!value) throw new Error("useLibrary must be used inside LibraryProvider");
  return value;
}

export function usePlaybackClock(): PlaybackClock {
  const clock = useContext(ClockContext);
  if (!clock) throw new Error("usePlaybackClock must be used inside LibraryProvider");
  return clock;
}

/**
 * The playback position, re-rendering only when it moves into a new step of
 * `granularityMs` (and at once after a seek, pause, or track change).
 */
export function usePosition(granularityMs = 1000): number {
  const clock = usePlaybackClock();
  const [position, setPosition] = useState(() => clock.position());
  useEffect(() => clock.subscribe(setPosition, granularityMs), [clock, granularityMs]);
  return position;
}

/**
 * Loads data for a view and reloads it only when one of the named parts of
 * the library changes.
 */
export function useLibraryData<T>(
  load: () => Promise<T>,
  deps: unknown[],
  scopes: Scope[],
): { data: T | null; error: string | null } {
  const { versions } = useLibrary();
  const stamp = scopes.map((scope) => versions[scope]).join(".");
  const [state, setState] = useState<{ data: T | null; error: string | null }>({
    data: null,
    error: null,
  });
  useEffect(() => {
    let cancelled = false;
    load()
      .then((data) => !cancelled && setState({ data, error: null }))
      .catch((error) => !cancelled && setState({ data: null, error: errorMessage(error) }));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stamp, ...deps]);
  return state;
}
