// Library shapes shared with the Rust core (src-tauri/src/library.rs,
// playback.rs, scanner.rs) and the helpers the views use.

import { t, tn } from "../i18n";

export interface Track {
  id: number;
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  genre: string | null;
  year: number | null;
  trackNo: number | null;
  durationMs: number;
  codec: string | null;
  bitrateKbps: number | null;
  sampleRate: number | null;
  bitDepth: number | null;
  channels: number | null;
  /** File name in the artwork cache. */
  artwork: string | null;
  /** Unix seconds. */
  addedAt: number;
  favorite: boolean;
  /** False when the file is missing, removed, or its folder was removed. */
  available: boolean;
  playCount: number;
  path: string;
  /** True when the user edited this track's details in BitNot. */
  edited: boolean;
  /** How many files in the library are this same recording. */
  sources: number;
}

export type CollectionKind = "playlist" | "album" | "artist";

export interface Collection {
  id: string;
  kind: CollectionKind;
  title: string;
  subtitle: string;
  artwork: string | null;
  trackCount: number;
  year: number | null;
  description: string;
  customCover: boolean;
}

export interface CollectionDetail extends Collection {
  owner: string;
  tracks: Track[];
  popular: Track[];
  albums: Collection[];
}

export interface Overview {
  trackCount: number;
  playlists: Collection[];
  albums: Collection[];
  artists: Collection[];
  favoriteIds: number[];
  issueCount: number;
  trashCount: number;
}

export interface HistoryEntry {
  id: number;
  playedAt: number;
  track: Track;
}

export interface SearchResults {
  query: string;
  songs: Track[];
  albums: Collection[];
  artists: Collection[];
}

export interface ScanIssue {
  path: string;
  reason: string;
}

export interface ScanSummary {
  added: number;
  updated: number;
  unchanged: number;
  moved: number;
  missing: number;
  issues: number;
  unavailableFolders: string[];
}

/** A details edit. `null` leaves a field as the file reports it. */
export interface TrackEdit {
  title: string | null;
  artist: string | null;
  album: string | null;
  genre: string | null;
  year: number | null;
  trackNo: number | null;
}

export type TrashKind = "playlist" | "favorite" | "track" | "metadata";

export interface TrashItem {
  id: number;
  kind: TrashKind;
  label: string;
  deletedAt: number;
  daysRemaining: number;
}

export type Repeat = "off" | "all" | "one";

export interface PlaybackState {
  track: Track | null;
  nextUp: Track[];
  nextUpTotal: number;
  positionMs: number;
  playing: boolean;
  shuffle: boolean;
  repeat: Repeat;
  volume: number;
  outputAvailable: boolean;
  error: string | null;
}

export interface Progress {
  trackId: number | null;
  positionMs: number;
  playing: boolean;
}

/** Which part of the library changed (payload of the `library:changed` event). */
export type Scope = "tracks" | "playlists" | "favorites" | "history" | "trash";

export const EMPTY_OVERVIEW: Overview = {
  trackCount: 0,
  playlists: [],
  albums: [],
  artists: [],
  favoriteIds: [],
  issueCount: 0,
  trashCount: 0,
};

export const EMPTY_PLAYBACK: PlaybackState = {
  track: null,
  nextUp: [],
  nextUpTotal: 0,
  positionMs: 0,
  playing: false,
  shuffle: false,
  repeat: "off",
  volume: 80,
  outputAvailable: true,
  error: null,
};

export const kindLabel = (kind: CollectionKind) => t(`kind.${kind}`);

/** `3:07` from milliseconds, or `1:02:45` from an hour up. */
export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const seconds = String(s % 60).padStart(2, "0");
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${seconds}`
    : `${minutes}:${seconds}`;
}

/** `3 hr 13 min`, `45 min`, or `under 1 min` for a whole collection. */
export function formatTotalDuration(ms: number): string {
  const totalMinutes = Math.floor(Math.max(0, ms) / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours > 0) return minutes > 0 ? `${hours} hr ${minutes} min` : `${hours} hr`;
  return minutes > 0 ? `${minutes} min` : "under 1 min";
}

export const formatSongCount = (count: number) => tn("count.songs", count);

const DATE_FORMAT = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
});

/** `Jan 1, 2026` from Unix seconds. */
export function formatDate(unixSeconds: number): string {
  return unixSeconds > 0 ? DATE_FORMAT.format(unixSeconds * 1000) : "";
}

/**
 * Short quality text for the list column, from what the file reports:
 * `FLAC · 44.1 kHz / 16-bit` for lossless, `MP3 · 320 kbps` otherwise.
 * It states the file's format only and makes no claim about the output.
 */
export function qualityText(track: Track): string {
  if (!track.codec) return "";
  const codec = track.codec.replace(" (MP4)", "");
  if (track.bitDepth && track.sampleRate) {
    return `${codec} · ${(track.sampleRate / 1000).toFixed(1)} kHz / ${track.bitDepth}-bit`;
  }
  return track.bitrateKbps ? `${codec} · ${track.bitrateKbps} kbps` : codec;
}

/** Technical details for Song Details. Only what the file actually reports. */
export function audioFacts(track: Track): { label: string; value: string }[] {
  const facts: { label: string; value: string }[] = [];
  if (track.codec) facts.push({ label: t("details.format"), value: track.codec });
  if (track.bitrateKbps) facts.push({ label: t("details.bitrate"), value: `${track.bitrateKbps} kbps` });
  if (track.sampleRate) {
    facts.push({ label: t("details.sampleRate"), value: `${(track.sampleRate / 1000).toFixed(1)} kHz` });
  }
  if (track.bitDepth) facts.push({ label: t("details.bitDepth"), value: `${track.bitDepth}-bit` });
  if (track.channels) {
    const names: Record<number, string> = { 1: t("details.mono"), 2: t("details.stereo") };
    facts.push({ label: t("details.channels"), value: names[track.channels] ?? `${track.channels}` });
  }
  return facts;
}

export function scanSummaryText(summary: ScanSummary): string {
  const parts: string[] = [];
  if (summary.added) parts.push(t("scan.added", { count: summary.added }));
  if (summary.updated) parts.push(t("scan.updated", { count: summary.updated }));
  if (summary.moved) parts.push(t("scan.moved", { count: summary.moved }));
  if (summary.missing) parts.push(t("scan.missing", { count: summary.missing }));
  if (summary.issues) parts.push(t("scan.skipped", { count: summary.issues }));
  if (parts.length === 0) {
    return summary.unchanged > 0 ? t("scan.upToDate") : t("scan.nothingFound");
  }
  return t("scan.finished", { parts: parts.join(", ") });
}

/** True when a scan changed what the library contains. */
export function scanChangedLibrary(summary: ScanSummary): boolean {
  return summary.added + summary.updated + summary.moved + summary.missing + summary.issues > 0;
}

// ── Rows: stable identity for filtering, sorting, and playback ───────────

export type SortField = "default" | "title" | "artist" | "album" | "duration" | "added";

export interface Sort {
  field: SortField;
  descending: boolean;
}

export const DEFAULT_SORT: Sort = { field: "default", descending: false };

/**
 * One displayed row.
 * - `sourceIndex` is the entry's position in the list as stored (the playlist
 *   order). It identifies the entry even when the same track appears twice and
 *   the view is filtered or sorted, so Remove and Move act on the right one.
 * - `playIndex` is its position in the sorted, unfiltered order, which is the
 *   order playback continues in.
 */
export interface TrackRow {
  track: Track;
  sourceIndex: number;
  playIndex: number;
}

function compare(field: SortField, a: Track, b: Track): number {
  const text = (x: string, y: string) => x.localeCompare(y, undefined, { sensitivity: "base" });
  switch (field) {
    case "title":
      return text(a.title, b.title);
    case "artist":
      return text(a.artist, b.artist) || text(a.album, b.album);
    case "album":
      return text(a.album, b.album);
    case "duration":
      return a.durationMs - b.durationMs;
    case "added":
      return a.addedAt - b.addedAt;
    case "default":
      return 0;
  }
}

/**
 * Builds the rows to display and the list playback should run through.
 * Sorting and filtering never change the stored order.
 */
export function buildRows(
  tracks: Track[],
  query: string,
  sort: Sort,
): { rows: TrackRow[]; playList: Track[] } {
  let ordered = tracks.map((track, sourceIndex) => ({ track, sourceIndex }));
  if (sort.field !== "default") {
    const direction = sort.descending ? -1 : 1;
    ordered = [...ordered].sort(
      (a, b) => direction * compare(sort.field, a.track, b.track) || a.sourceIndex - b.sourceIndex,
    );
  } else if (sort.descending) {
    ordered = [...ordered].reverse();
  }
  const needle = query.trim().toLowerCase();
  const rows: TrackRow[] = [];
  ordered.forEach(({ track, sourceIndex }, playIndex) => {
    if (
      !needle ||
      [track.title, track.artist, track.album].some((field) => field.toLowerCase().includes(needle))
    ) {
      rows.push({ track, sourceIndex, playIndex });
    }
  });
  return { rows, playList: ordered.map((entry) => entry.track) };
}

/** Next sort when a field is chosen: ascending, then descending. */
export function nextSort(current: Sort, field: SortField): Sort {
  if (field === "default") return DEFAULT_SORT;
  return { field, descending: current.field === field ? !current.descending : false };
}

// ── Virtual list ─────────────────────────────────────────────────────────

/**
 * Which rows of a fixed-height list need to be in the page.
 * `listTop` is the list's offset inside the scrolling element.
 */
export function visibleRange(
  scrollTop: number,
  viewportHeight: number,
  listTop: number,
  rowHeight: number,
  count: number,
  overscan: number,
): { start: number; end: number } {
  if (count === 0 || rowHeight <= 0) return { start: 0, end: 0 };
  const first = Math.floor((scrollTop - listTop) / rowHeight) - overscan;
  const last = Math.ceil((scrollTop - listTop + viewportHeight) / rowHeight) + overscan;
  const start = Math.min(Math.max(0, first), count);
  return { start, end: Math.min(count, Math.max(start, last)) };
}

/** Moves one item of a list (drag-and-drop, Move up/down). */
export function moved<T>(list: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || to < 0 || from >= list.length || to >= list.length) return list;
  const copy = [...list];
  const [item] = copy.splice(from, 1);
  copy.splice(to, 0, item);
  return copy;
}
