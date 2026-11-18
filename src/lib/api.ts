import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  CollectionDetail,
  HistoryEntry,
  Overview,
  PlaybackState,
  Repeat,
  ScanIssue,
  ScanSummary,
  SearchResults,
  Track,
  TrackEdit,
  TrashItem,
} from "./library";
import type { AppErrorPayload, Bootstrap, LogFile, Settings } from "./types";

/** Turns anything thrown by the core into a message fit to show the user. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as AppErrorPayload).message);
  }
  return "Something went wrong.";
}

export const EVENTS = {
  progress: "playback:progress",
  playbackChanged: "playback:changed",
  libraryChanged: "library:changed",
  scanProgress: "scan:progress",
  scanFinished: "scan:finished",
} as const;

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  updateSettings: (patch: Partial<Settings>) =>
    invoke<Settings>("settings_update", { patch }),
  completeSetup: () => invoke<Bootstrap>("setup_complete"),

  createProfile: (name: string) => invoke<Bootstrap>("profile_create", { name }),
  renameProfile: (id: string, name: string) =>
    invoke<Bootstrap>("profile_rename", { id, name }),
  switchProfile: (id: string) => invoke<Bootstrap>("profile_switch", { id }),
  setMainProfile: (id: string) => invoke<Bootstrap>("profile_set_main", { id }),
  deleteProfile: (id: string) => invoke<Bootstrap>("profile_delete", { id }),
  restoreProfile: (id: string) => invoke<Bootstrap>("profile_restore", { id }),
  purgeProfile: (id: string) => invoke<Bootstrap>("profile_purge", { id }),

  listLogs: () => invoke<LogFile[]>("diagnostics_list"),
  readLog: (name: string) => invoke<string>("diagnostics_read", { name }),
  clearLogs: () => invoke<number>("diagnostics_clear"),

  overview: () => invoke<Overview>("library_overview"),
  tracks: () => invoke<Track[]>("library_tracks"),
  recentTracks: () => invoke<Track[]>("library_recent"),
  genres: () => invoke<string[]>("library_genres"),
  trackSources: (id: number) => invoke<Track[]>("track_sources", { id }),
  preferSource: (id: number) => invoke<void>("track_prefer_source", { id }),
  editTrack: (id: number, edit: TrackEdit) => invoke<Track>("track_edit", { id, edit }),
  setTrackArtwork: (trackIds: number[], image: string | null) =>
    invoke<void>("track_set_artwork", { trackIds, image }),
  removeTracks: (trackIds: number[]) => invoke<number>("tracks_remove", { trackIds }),
  trash: () => invoke<TrashItem[]>("trash_list"),
  restoreTrash: (id: number) => invoke<void>("trash_restore", { id }),
  purgeTrash: (id: number) => invoke<void>("trash_purge", { id }),
  emptyTrash: () => invoke<number>("trash_empty"),
  collection: (id: string) => invoke<CollectionDetail>("library_collection", { id }),
  favorites: () => invoke<Track[]>("library_favorites"),
  history: () => invoke<HistoryEntry[]>("library_history"),
  removeHistoryEntry: (id: number) => invoke<void>("history_remove", { id }),
  clearHistory: () => invoke<void>("history_clear"),
  search: (query: string) => invoke<SearchResults>("library_search", { query }),
  scanIssues: () => invoke<ScanIssue[]>("library_issues"),
  scan: () => invoke<ScanSummary>("library_scan"),
  setFavorite: (trackId: number, on: boolean) => invoke<void>("favorite_set", { trackId, on }),

  createPlaylist: (name: string) => invoke<string>("playlist_create", { name }),
  updatePlaylist: (id: string, name: string, description: string) =>
    invoke<void>("playlist_update", { id, name, description }),
  setPlaylistCover: (id: string, image: string | null) =>
    invoke<void>("playlist_set_cover", { id, image }),
  movePlaylistTrack: (id: string, from: number, to: number) =>
    invoke<void>("playlist_move", { id, from, to }),
  playlistFromQueue: (name: string) => invoke<string>("playlist_from_queue", { name }),
  deletePlaylist: (id: string) => invoke<void>("playlist_delete", { id }),
  addToPlaylist: (id: string, trackIds: number[]) =>
    invoke<void>("playlist_add", { id, trackIds }),
  removeFromPlaylist: (id: string, index: number) =>
    invoke<void>("playlist_remove", { id, index }),

  playbackState: () => invoke<PlaybackState>("playback_state"),
  playTracks: (trackIds: number[], start: number) =>
    invoke<PlaybackState>("play_tracks", { trackIds, start }),
  toggle: () => invoke<PlaybackState>("playback_toggle"),
  next: () => invoke<PlaybackState>("playback_next"),
  previous: () => invoke<PlaybackState>("playback_previous"),
  seek: (positionMs: number) => invoke<PlaybackState>("playback_seek", { positionMs }),
  setShuffle: (on: boolean) => invoke<PlaybackState>("playback_set_shuffle", { on }),
  setRepeat: (repeat: Repeat) => invoke<PlaybackState>("playback_set_repeat", { repeat }),
  queuePlayNext: (trackId: number) => invoke<PlaybackState>("queue_play_next", { trackId }),
  queueAdd: (trackId: number) => invoke<PlaybackState>("queue_add", { trackId }),
  queueAddMany: (trackIds: number[]) => invoke<PlaybackState>("queue_add_many", { trackIds }),
  queueMove: (from: number, to: number) => invoke<PlaybackState>("queue_move", { from, to }),
  setVolumeLive: (percent: number) => invoke<void>("playback_set_volume", { percent }),
  queueRemove: (offset: number) => invoke<PlaybackState>("queue_remove", { offset }),
  queueClear: () => invoke<PlaybackState>("queue_clear"),
  queuePlay: (offset: number) => invoke<PlaybackState>("queue_play", { offset }),

  /** Opens the Windows file picker for a cover image. Resolves to null when cancelled. */
  async pickImage(): Promise<string | null> {
    const picked = await open({
      directory: false,
      multiple: false,
      title: "Choose a cover image",
      filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png", "webp", "gif", "bmp"] }],
    });
    return typeof picked === "string" ? picked : null;
  },

  /** Opens the Windows folder picker. Resolves to null when cancelled. */
  async pickFolder(): Promise<string | null> {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose a music folder",
    });
    return typeof picked === "string" ? picked : null;
  },
};
