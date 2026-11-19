// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_OVERVIEW, type Track } from "../lib/library";
import { UiProvider, useUi, type TrackContext } from "./UiContext";

const library = {
  overview: {
    ...EMPTY_OVERVIEW,
    playlists: [
      { id: "playlist:1", kind: "playlist", title: "Road Trip", subtitle: "Playlist", artwork: null, trackCount: 0, year: null, description: "", customCover: false },
    ],
  },
  versions: { tracks: 0, playlists: 0, favorites: 0, history: 0, trash: 0 },
  isFavorite: () => false,
  playTracks: vi.fn(),
  playNext: vi.fn(),
  addToQueue: vi.fn(),
  addToPlaylist: vi.fn(),
  setFavorite: vi.fn(),
  removeFromPlaylist: vi.fn(),
  removeFromLibrary: vi.fn(),
  createPlaylist: vi.fn(() => Promise.resolve(null)),
  saveQueueAsPlaylist: vi.fn(() => Promise.resolve(null)),
  deletePlaylist: vi.fn(() => Promise.resolve(true)),
  startScan: vi.fn(),
  artworkUrl: () => undefined,
};
const app = { go: vi.fn(), notify: vi.fn(), route: { page: "home" }, settings: {}, updateSettings: vi.fn() };

vi.mock("./LibraryContext", () => ({ useLibrary: () => library }));
vi.mock("./AppContext", () => ({ useApp: () => app }));
vi.mock("../lib/api", () => ({ api: {}, errorMessage: String }));

const track = { id: 5, title: "Night Bus", artist: "Mira", album: "Low Tide", available: true, sources: 1 } as unknown as Track;

let ui!: ReturnType<typeof useUi>;
function Probe() {
  ui = useUi();
  return <button>origin</button>;
}

beforeAll(() => {
  // jsdom does not implement modal dialogs.
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
beforeEach(() => {
  vi.clearAllMocks();
  render(
    <UiProvider>
      <Probe />
    </UiProvider>,
  );
});
afterEach(cleanup);

const open = (context: Partial<TrackContext> = {}) =>
  act(() => ui.openTrackMenu(track, { list: [track], index: 0, ...context }, 10, 10));
const items = () => screen.getAllByRole("menuitem").map((item) => item.textContent);

describe("menu choices", () => {
  it("marks a group of choices as radio items with the chosen one checked", () => {
    const pick = vi.fn();
    act(() =>
      ui.openMenu(
        [
          { label: "15 minutes", checked: false, onSelect: pick },
          { label: "30 minutes", checked: true },
          { separator: true },
          { label: "Turn off" },
        ],
        10,
        10,
      ),
    );
    const radios = screen.getAllByRole("menuitemradio");
    expect(radios.map((item) => [item.textContent, item.getAttribute("aria-checked")])).toEqual([
      ["15 minutes", "false"],
      ["30 minutes", "true"],
    ]);
    // Plain actions stay ordinary menu items.
    expect(screen.getByRole("menuitem").textContent).toBe("Turn off");
    fireEvent.click(radios[0]);
    expect(pick).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("track menu", () => {
  it("offers the actions from the specification, including Remove from Library", () => {
    open();
    expect(items()).toEqual([
      "Play",
      "Play Next",
      "Add to Queue",
      "Add to Playlist",
      "Add to Favorites",
      "View Details",
      "Edit Details…",
      "Remove from Library…",
    ]);
  });

  it("is operated with the keyboard and returns focus when closed", () => {
    const origin = screen.getByText("origin");
    origin.focus();
    open();
    const menu = screen.getByRole("menu");
    expect(document.activeElement?.textContent).toBe("Play");
    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Play Next");
    fireEvent.keyDown(window, { key: "End" });
    expect(document.activeElement?.textContent).toBe("Remove from Library…");
    fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toBe("Play");
    expect(menu).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("adds to an existing playlist through the second level", () => {
    open();
    fireEvent.click(screen.getByText("Add to Playlist"));
    expect(items()).toEqual(["Back", "New playlist…", "Road Trip"]);
    fireEvent.click(screen.getByText("Road Trip"));
    expect(library.addToPlaylist).toHaveBeenCalledWith("playlist:1", [track], "Road Trip");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("removes and moves the exact playlist entry it was opened on", () => {
    const onMove = vi.fn();
    open({ playlistId: "playlist:1", entryIndex: 2, entryCount: 4, onMove });
    expect(items()).toContain("Move up");
    fireEvent.click(screen.getByText("Move down"));
    expect(onMove).toHaveBeenCalledWith(2, 3);

    open({ playlistId: "playlist:1", entryIndex: 2, entryCount: 4, onMove });
    fireEvent.click(screen.getByText("Remove from this playlist"));
    expect(library.removeFromPlaylist).toHaveBeenCalledWith("playlist:1", 2);

    // The first entry cannot move up; the last cannot move down.
    open({ playlistId: "playlist:1", entryIndex: 0, entryCount: 1, onMove });
    expect((screen.getByText("Move up").closest("button") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText("Move down").closest("button") as HTMLButtonElement).disabled).toBe(true);
  });

  it("asks before removing from the library and says files are not touched", () => {
    open();
    fireEvent.click(screen.getByText("Remove from Library…"));
    expect(library.removeFromLibrary).not.toHaveBeenCalled();
    const dialog = screen.getByText("Remove this track from your library?").closest("dialog");
    expect(dialog?.textContent).toContain("The audio file stays exactly where it is");
    fireEvent.click(screen.getByRole("button", { name: "Remove from Library" }));
    expect(library.removeFromLibrary).toHaveBeenCalledWith([track]);
  });

  it("does not offer playback for a track whose file is missing", () => {
    act(() =>
      ui.openTrackMenu({ ...track, available: false } as Track, { list: [track], index: 0 }, 10, 10),
    );
    const disabled = screen
      .getAllByRole("menuitem")
      .filter((item) => (item as HTMLButtonElement).disabled)
      .map((item) => item.textContent);
    expect(disabled).toEqual(["Play", "Play Next", "Add to Queue"]);
  });
});
