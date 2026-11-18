// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_OVERVIEW, EMPTY_PLAYBACK, type Scope, type Track } from "../lib/library";
import { LibraryProvider, thumbName, useLibrary, useLibraryData } from "./LibraryContext";

const notify = vi.fn();
const listeners: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (path: string) => `asset://${path}` }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners[name] = handler;
    return Promise.resolve(() => undefined);
  },
}));
vi.mock("./AppContext", () => ({
  useApp: () => ({
    notify,
    boot: {
      activeProfileId: "p1",
      setupComplete: true,
      artworkDir: "C:\\art",
      settings: { musicFolders: [], volume: 80 },
    },
  }),
}));
vi.mock("../lib/api", () => {
  const ok = <T,>(value: T) => vi.fn(() => Promise.resolve(value));
  return {
    errorMessage: (error: unknown) => String(error),
    EVENTS: {
      progress: "playback:progress",
      playbackChanged: "playback:changed",
      libraryChanged: "library:changed",
      scanProgress: "scan:progress",
      scanFinished: "scan:finished",
    },
    api: {
      overview: ok({ ...EMPTY_OVERVIEW, favoriteIds: [7] }),
      playbackState: ok(EMPTY_PLAYBACK),
      queueAddMany: ok(EMPTY_PLAYBACK),
      setVolumeLive: ok(undefined),
      updateSettings: ok({}),
      setFavorite: ok(undefined),
      favorites: ok([]),
      tracks: ok([]),
    },
  };
});

import { api } from "../lib/api";

const track = (id: number, title: string, available = true) =>
  ({ id, title, available }) as unknown as Track;

let library!: ReturnType<typeof useLibrary>;
function Probe() {
  library = useLibrary();
  return null;
}

async function mount(extra?: React.ReactNode) {
  render(
    <LibraryProvider>
      <Probe />
      {extra}
    </LibraryProvider>,
  );
  await act(async () => {});
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("LibraryProvider", () => {
  // Regression (audit defect 2): one request and one notification per batch.
  it("adds many tracks to the queue with one request and one notification", async () => {
    await mount();
    act(() => library.addToQueue([track(1, "A"), track(2, "B", false), track(3, "C")]));
    expect(api.queueAddMany).toHaveBeenCalledTimes(1);
    expect(api.queueAddMany).toHaveBeenCalledWith([1, 3]);
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify).toHaveBeenCalledWith("Added 2 songs to the queue.");

    act(() => library.addToQueue([track(4, "Solo")]));
    expect(notify).toHaveBeenLastCalledWith("Added “Solo” to the queue.");
    act(() => library.addToQueue([track(5, "Gone", false)]));
    expect(api.queueAddMany).toHaveBeenCalledTimes(2);
    expect(notify).toHaveBeenLastCalledWith(expect.stringContaining("missing"), "error");
  });

  it("applies volume at once but writes settings only when the slider rests", async () => {
    await mount();
    vi.useFakeTimers();
    act(() => {
      for (const value of [70, 60, 50, 40]) library.setVolume(value);
    });
    expect(library.volume).toBe(40);
    expect(api.setVolumeLive).toHaveBeenCalledTimes(4);
    expect(api.updateSettings).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(500));
    expect(api.updateSettings).toHaveBeenCalledTimes(1);
    expect(api.updateSettings).toHaveBeenCalledWith({ volume: 40 });
  });

  it("shows a favorite change at once and does not reload the library for it", async () => {
    await mount();
    expect(library.isFavorite(7)).toBe(true);
    expect(library.favoriteCount).toBe(1);
    act(() => library.setFavorite(track(9, "New"), true));
    expect(library.isFavorite(9)).toBe(true);
    expect(api.setFavorite).toHaveBeenCalledWith(9, true);

    const before = vi.mocked(api.overview).mock.calls.length;
    await act(async () => listeners["library:changed"]({ payload: "favorites" satisfies Scope }));
    await act(async () => listeners["library:changed"]({ payload: "history" satisfies Scope }));
    expect(vi.mocked(api.overview).mock.calls.length).toBe(before);
    await act(async () => listeners["library:changed"]({ payload: "tracks" satisfies Scope }));
    expect(vi.mocked(api.overview).mock.calls.length).toBe(before + 1);
  });

  it("reloads a view only for the parts of the library it shows", async () => {
    function FavoritesView() {
      useLibraryData(api.favorites, [], ["favorites"]);
      return null;
    }
    function SongsView() {
      useLibraryData(api.tracks, [], ["tracks"]);
      return null;
    }
    await mount(
      <>
        <FavoritesView />
        <SongsView />
      </>,
    );
    const favoritesLoads = vi.mocked(api.favorites).mock.calls.length;
    const tracksLoads = vi.mocked(api.tracks).mock.calls.length;
    await act(async () => listeners["library:changed"]({ payload: "favorites" }));
    await act(async () => listeners["library:changed"]({ payload: "history" }));
    expect(vi.mocked(api.favorites).mock.calls.length).toBe(favoritesLoads + 1);
    expect(vi.mocked(api.tracks).mock.calls.length).toBe(tracksLoads);
  });

  it("builds artwork addresses and thumbnail names", async () => {
    await mount();
    expect(thumbName("abc123.jpg")).toBe("abc123.thumb.jpg");
    expect(thumbName("abc123.anim.gif")).toBe("abc123.thumb.jpg");
    expect(library.artworkUrl("abc.png")).toBe("asset://C:\\art\\abc.png");
    expect(library.artworkUrl("abc.png", true)).toBe("asset://C:\\art\\abc.thumb.jpg");
    expect(library.artworkUrl(null)).toBeUndefined();
  });
});
