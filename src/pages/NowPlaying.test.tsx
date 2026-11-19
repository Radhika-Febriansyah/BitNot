// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PlayerBar } from "../components/PlayerBar";
import { EMPTY_PLAYBACK, type PlaybackState, type Track } from "../lib/library";
import type { Route } from "../lib/router";
import { albumPageId, artistPageId, NEXT_UP_SHOWN, NowPlaying } from "./NowPlaying";

const track = (id: number, title: string, over: Partial<Track> = {}): Track => ({
  id,
  title,
  artist: "Mira",
  album: "Low Tide",
  albumArtist: "Mira",
  genre: null,
  year: null,
  trackNo: null,
  durationMs: 200_000,
  codec: "FLAC",
  bitrateKbps: null,
  sampleRate: 44100,
  bitDepth: 16,
  channels: 2,
  artwork: null,
  addedAt: 1_780_000_000,
  favorite: false,
  available: true,
  playCount: 0,
  path: `C:\\Music\\${title}.flac`,
  edited: false,
  sources: 1,
  ...over,
});

const current = track(1, "Night Bus");
const upcoming = Array.from({ length: 8 }, (_, i) => track(10 + i, `Next ${i + 1}`));

let favorites = new Set<number>();
const library = {
  playback: EMPTY_PLAYBACK as PlaybackState,
  artworkUrl: (name: string | null | undefined, thumb?: boolean) =>
    name ? `asset://${name}${thumb ? ".thumb" : ""}` : undefined,
  isFavorite: (id: number) => favorites.has(id),
  setFavorite: vi.fn(),
  seek: vi.fn(),
  toggle: vi.fn(),
  next: vi.fn(),
  previous: vi.fn(),
  setShuffle: vi.fn(),
  setRepeat: vi.fn(),
  volume: 60,
  setVolume: vi.fn(),
  muted: false,
  toggleMute: vi.fn(),
};
const app = {
  route: { page: "nowPlaying" } as Route,
  settings: { nowPlayingLayout: "centered" },
  updateSettings: vi.fn(() => Promise.resolve()),
  go: vi.fn(),
  back: vi.fn(),
  canBack: true,
};
const ui = { showDetails: vi.fn() };

vi.mock("../state/LibraryContext", () => ({ useLibrary: () => library, usePosition: () => 30_000 }));
vi.mock("../state/AppContext", () => ({ useApp: () => app }));
vi.mock("../state/UiContext", () => ({ useUi: () => ui }));

function playing(over: Partial<PlaybackState> = {}) {
  library.playback = {
    ...EMPTY_PLAYBACK,
    track: { ...current, artwork: "cover.jpg" },
    nextUp: upcoming,
    nextUpTotal: upcoming.length + 4,
    playing: true,
    outputAvailable: true,
    ...over,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  favorites = new Set();
  library.playback = { ...EMPTY_PLAYBACK, outputAvailable: true };
  app.route = { page: "nowPlaying" };
  app.settings = { nowPlayingLayout: "centered" };
  app.canBack = true;
});
afterEach(cleanup);

describe("NowPlaying", () => {
  it("invites the user to pick something when nothing is playing", () => {
    render(<NowPlaying />);
    expect(screen.getByRole("heading", { name: "Nothing playing" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Browse your library" }));
    expect(app.go).toHaveBeenCalledWith({ page: "library", filter: "all" });
  });

  it("shows the playing track with full-size artwork and working controls", () => {
    playing();
    const { container } = render(<NowPlaying />);
    expect(screen.getByRole("heading", { name: "Night Bus" })).toBeTruthy();
    expect(container.querySelector(".np-art img")?.getAttribute("src")).toBe("asset://cover.jpg");
    expect(screen.getByText("FLAC · 44.1 kHz / 16-bit")).toBeTruthy();
    expect(screen.getByRole("slider", { name: "Playback position" }).getAttribute("aria-valuetext")).toBe(
      "0:30 of 3:20",
    );

    fireEvent.click(screen.getByRole("button", { name: "Pause" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Previous" }));
    fireEvent.click(screen.getByRole("button", { name: "Shuffle" }));
    fireEvent.click(screen.getByRole("button", { name: "Mute" }));
    expect(library.toggle).toHaveBeenCalledTimes(1);
    expect(library.next).toHaveBeenCalledTimes(1);
    expect(library.previous).toHaveBeenCalledTimes(1);
    expect(library.setShuffle).toHaveBeenCalledWith(true);
    expect(library.toggleMute).toHaveBeenCalledTimes(1);
  });

  it("links to the artist and album pages and opens Song details", () => {
    playing();
    render(<NowPlaying />);
    fireEvent.click(screen.getByRole("button", { name: "Mira" }));
    expect(app.go).toHaveBeenLastCalledWith({ page: "collection", id: "artist:Mira" });
    fireEvent.click(screen.getByRole("button", { name: "Low Tide" }));
    expect(app.go).toHaveBeenLastCalledWith({ page: "collection", id: "album:Mira\u001fLow Tide" });
    fireEvent.click(screen.getByRole("button", { name: "Song details" }));
    expect(ui.showDetails).toHaveBeenCalledWith(
      expect.objectContaining({ id: 1 }),
      expect.objectContaining({ index: 0 }),
    );
  });

  it("adds and removes the favorite with a focusable toggle", () => {
    playing();
    const view = render(<NowPlaying />);
    const add = screen.getByRole("button", { name: /add .*night bus.* to favorites/i });
    expect(add.getAttribute("aria-pressed")).toBe("false");
    expect(add.tabIndex).toBe(0);
    fireEvent.click(add);
    expect(library.setFavorite).toHaveBeenLastCalledWith(expect.objectContaining({ id: 1 }), true);

    favorites.add(1);
    view.rerender(<NowPlaying />);
    const remove = screen.getByRole("button", { name: /remove .*night bus.* from favorites/i });
    expect(remove.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(remove);
    expect(library.setFavorite).toHaveBeenLastCalledWith(expect.objectContaining({ id: 1 }), false);
  });

  it("saves the chosen layout and lists what plays next only in Split", () => {
    playing();
    const view = render(<NowPlaying />);
    const group = screen.getByRole("radiogroup", { name: "Now Playing layout" });
    expect(within(group).getByRole("radio", { name: "Centered" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.queryByRole("heading", { name: "Next up" })).toBeNull();
    fireEvent.click(within(group).getByRole("radio", { name: "Split" }));
    expect(app.updateSettings).toHaveBeenCalledWith({ nowPlayingLayout: "split" });

    app.settings = { nowPlayingLayout: "split" };
    view.rerender(<NowPlaying />);
    expect(view.container.querySelector(".now-playing")?.getAttribute("data-layout")).toBe("split");
    const next = screen.getByRole("heading", { name: "Next up" }).closest("section")!;
    expect(within(next).getAllByRole("listitem")).toHaveLength(NEXT_UP_SHOWN);
    expect(within(next).getByText("Next 1")).toBeTruthy();
    // 12 in the queue, 5 listed.
    expect(within(next).getByText("and 7 more")).toBeTruthy();
  });

  it("says when nothing is queued after the current track", () => {
    playing({ nextUp: [], nextUpTotal: 0 });
    app.settings = { nowPlayingLayout: "split" };
    render(<NowPlaying />);
    expect(screen.getByText("Nothing is queued after this track.")).toBeTruthy();
  });

  it("builds the same page ids as the core", () => {
    expect(artistPageId({ artist: "Mira" })).toBe("artist:Mira");
    expect(albumPageId({ albumArtist: "Various Artists", album: "Hits" })).toBe("album:Various Artists\u001fHits");
  });
});

describe("PlayerBar", () => {
  it("opens Now Playing from the current track and goes back from there", () => {
    playing();
    app.route = { page: "home" };
    const view = render(<PlayerBar queueOpen={false} onToggleQueue={vi.fn()} />);
    const open = screen.getByRole("button", { name: "Open Now Playing" });
    expect(open.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(open);
    expect(app.go).toHaveBeenCalledWith({ page: "nowPlaying" });

    app.route = { page: "nowPlaying" };
    view.rerender(<PlayerBar queueOpen={false} onToggleQueue={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Close Now Playing" }));
    expect(app.back).toHaveBeenCalledTimes(1);

    // Restored straight onto Now Playing with no history: go Home instead.
    app.canBack = false;
    view.rerender(<PlayerBar queueOpen={false} onToggleQueue={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Close Now Playing" }));
    expect(app.go).toHaveBeenLastCalledWith({ page: "home" });
  });

  it("keeps the transport disabled when there is nothing to play", () => {
    render(<PlayerBar queueOpen={false} onToggleQueue={vi.fn()} />);
    expect((screen.getByRole("button", { name: "Play" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("slider", { name: "Playback position" }) as HTMLInputElement).disabled).toBe(true);
  });
});
