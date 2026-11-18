import { describe, expect, it } from "vitest";
import {
  audioFacts,
  buildRows,
  DEFAULT_SORT,
  formatDate,
  formatDuration,
  formatSongCount,
  formatTotalDuration,
  moved,
  nextSort,
  qualityText,
  scanChangedLibrary,
  scanSummaryText,
  visibleRange,
  type ScanSummary,
  type Track,
} from "./library";

export const track = (id: number, title: string, artist: string, album: string, over: Partial<Track> = {}): Track => ({
  id,
  title,
  artist,
  album,
  albumArtist: artist,
  genre: null,
  year: null,
  trackNo: null,
  durationMs: 180_000,
  codec: null,
  bitrateKbps: null,
  sampleRate: null,
  bitDepth: null,
  channels: null,
  artwork: null,
  addedAt: 0,
  favorite: false,
  available: true,
  playCount: 0,
  path: `C:\\Music\\${title}.mp3`,
  edited: false,
  sources: 1,
  ...over,
});

const summary = (over: Partial<ScanSummary>): ScanSummary => ({
  added: 0,
  updated: 0,
  unchanged: 0,
  moved: 0,
  missing: 0,
  issues: 0,
  unavailableFolders: [],
  ...over,
});

describe("library formatting", () => {
  it("formats track durations from milliseconds", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(180_000)).toBe("3:00");
    expect(formatDuration(291_900)).toBe("4:51");
    expect(formatDuration(3_765_000)).toBe("1:02:45");
    expect(formatDuration(-5)).toBe("0:00");
  });

  it("formats collection totals", () => {
    expect(formatTotalDuration(11_580_000)).toBe("3 hr 13 min");
    expect(formatTotalDuration(7_200_000)).toBe("2 hr");
    expect(formatTotalDuration(2_700_000)).toBe("45 min");
    expect(formatTotalDuration(20_000)).toBe("under 1 min");
  });

  it("formats counts and dates", () => {
    expect(formatSongCount(1)).toBe("1 song");
    expect(formatSongCount(40)).toBe("40 songs");
    expect(formatDate(0)).toBe("");
    expect(formatDate(Date.UTC(2026, 5, 15, 12) / 1000)).toBe("Jun 15, 2026");
  });

  it("lists only the audio facts a file reports", () => {
    expect(audioFacts(track(1, "a", "b", "c"))).toEqual([]);
    const flac = track(1, "a", "b", "c", { codec: "FLAC", sampleRate: 44100, bitDepth: 16, channels: 2 });
    expect(audioFacts(flac)).toEqual([
      { label: "Format", value: "FLAC" },
      { label: "Sample rate", value: "44.1 kHz" },
      { label: "Bit depth", value: "16-bit" },
      { label: "Channels", value: "Stereo" },
    ]);
  });

  it("describes quality from the file's own properties", () => {
    expect(qualityText(track(1, "a", "b", "c"))).toBe("");
    expect(qualityText(track(1, "a", "b", "c", { codec: "FLAC", sampleRate: 96000, bitDepth: 24 }))).toBe(
      "FLAC · 96.0 kHz / 24-bit",
    );
    expect(qualityText(track(1, "a", "b", "c", { codec: "MP3", bitrateKbps: 320, sampleRate: 44100 }))).toBe(
      "MP3 · 320 kbps",
    );
    expect(qualityText(track(1, "a", "b", "c", { codec: "AAC/ALAC (MP4)", bitrateKbps: 256 }))).toBe(
      "AAC/ALAC · 256 kbps",
    );
  });

  it("summarises a scan", () => {
    expect(scanSummaryText(summary({}))).toBe("No audio files were found.");
    expect(scanSummaryText(summary({ unchanged: 12 }))).toBe("Library is up to date.");
    expect(scanSummaryText(summary({ added: 3, issues: 1, moved: 2, unchanged: 5 }))).toBe(
      "Scan finished: 3 added, 2 moved, 1 skipped.",
    );
    expect(scanChangedLibrary(summary({ unchanged: 9 }))).toBe(false);
    expect(scanChangedLibrary(summary({ moved: 1 }))).toBe(true);
  });
});

describe("buildRows", () => {
  const a = track(1, "Harbor Lights", "Mira Solen", "Low Tide", { durationMs: 200_000, addedAt: 30 });
  const b = track(2, "Paper Kites", "The Oran Trio", "Harbor Sessions", { durationMs: 100_000, addedAt: 10 });
  const c = track(3, "Night Bus", "Mira Solen", "Low Tide", { durationMs: 300_000, addedAt: 20 });
  const tracks = [a, b, c];
  const titles = (rows: { track: Track }[]) => rows.map((r) => r.track.title);

  it("keeps stored order by default", () => {
    const { rows, playList } = buildRows(tracks, "", DEFAULT_SORT);
    expect(rows.map((r) => [r.sourceIndex, r.playIndex])).toEqual([[0, 0], [1, 1], [2, 2]]);
    expect(playList).toEqual(tracks);
  });

  it("filters on title, artist, or album without guessing at typos", () => {
    expect(titles(buildRows(tracks, "harbor", DEFAULT_SORT).rows)).toEqual(["Harbor Lights", "Paper Kites"]);
    expect(titles(buildRows(tracks, "MIRA", DEFAULT_SORT).rows)).toEqual(["Harbor Lights", "Night Bus"]);
    expect(buildRows(tracks, "harbour", DEFAULT_SORT).rows).toEqual([]);
  });

  it("sorts by each field in both directions and can return to the default", () => {
    const by = (field: Parameters<typeof nextSort>[1], descending = false) =>
      titles(buildRows(tracks, "", { field, descending }).rows);
    expect(by("title")).toEqual(["Harbor Lights", "Night Bus", "Paper Kites"]);
    expect(by("title", true)).toEqual(["Paper Kites", "Night Bus", "Harbor Lights"]);
    expect(by("artist")).toEqual(["Harbor Lights", "Night Bus", "Paper Kites"]);
    expect(by("duration")).toEqual(["Paper Kites", "Harbor Lights", "Night Bus"]);
    expect(by("added", true)).toEqual(["Harbor Lights", "Night Bus", "Paper Kites"]);
    expect(by("default")).toEqual(["Harbor Lights", "Paper Kites", "Night Bus"]);
    expect(tracks.map((t) => t.id)).toEqual([1, 2, 3]); // the stored list is not reordered
  });

  it("cycles a field ascending then descending, and resets", () => {
    let sort = nextSort(DEFAULT_SORT, "title");
    expect(sort).toEqual({ field: "title", descending: false });
    sort = nextSort(sort, "title");
    expect(sort).toEqual({ field: "title", descending: true });
    expect(nextSort(sort, "duration")).toEqual({ field: "duration", descending: false });
    expect(nextSort(sort, "default")).toEqual(DEFAULT_SORT);
  });

  // Regression: with the same track twice and a filter active, the second copy
  // used to resolve to the first copy's position.
  it("gives every entry its own identity when a track appears twice", () => {
    const playlist = [a, b, a, c];
    const { rows } = buildRows(playlist, "harbor lights", DEFAULT_SORT);
    expect(rows.map((r) => r.sourceIndex)).toEqual([0, 2]);
    expect(rows.map((r) => r.playIndex)).toEqual([0, 2]);

    const sorted = buildRows(playlist, "harbor lights", { field: "title", descending: false });
    expect(sorted.rows.map((r) => r.sourceIndex)).toEqual([0, 2]);
    expect(sorted.rows.map((r) => r.playIndex)).toEqual([0, 1]);
    expect(sorted.playList[sorted.rows[1].playIndex]).toBe(a);
  });
});

describe("visibleRange", () => {
  it("covers the viewport plus overscan and stays inside the list", () => {
    expect(visibleRange(0, 560, 0, 56, 1000, 5)).toEqual({ start: 0, end: 15 });
    expect(visibleRange(5600, 560, 0, 56, 1000, 5)).toEqual({ start: 95, end: 115 });
    expect(visibleRange(56_000, 560, 0, 56, 1000, 5)).toEqual({ start: 995, end: 1000 });
  });

  it("accounts for content above the list", () => {
    expect(visibleRange(0, 560, 400, 56, 1000, 0)).toEqual({ start: 0, end: 3 });
    expect(visibleRange(400, 560, 400, 56, 1000, 0)).toEqual({ start: 0, end: 10 });
  });

  it("handles empty lists and lists scrolled out of view", () => {
    expect(visibleRange(0, 500, 0, 56, 0, 5)).toEqual({ start: 0, end: 0 });
    expect(visibleRange(100_000, 500, 0, 56, 10, 2)).toEqual({ start: 10, end: 10 });
  });
});

describe("moved", () => {
  it("moves one item and leaves the original list alone", () => {
    const list = ["a", "b", "c", "d"];
    expect(moved(list, 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(moved(list, 3, 0)).toEqual(["d", "a", "b", "c"]);
    expect(moved(list, 1, 1)).toBe(list);
    expect(moved(list, 1, 9)).toBe(list);
    expect(list).toEqual(["a", "b", "c", "d"]);
  });
});
