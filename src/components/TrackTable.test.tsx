// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildRows, DEFAULT_SORT, EMPTY_PLAYBACK, type Track } from "../lib/library";
import { TrackTable, VIRTUAL_THRESHOLD } from "./TrackTable";

const library = {
  playback: EMPTY_PLAYBACK,
  artworkUrl: () => undefined,
  playTracks: vi.fn(),
  toggle: vi.fn(),
  setFavorite: vi.fn(),
  isFavorite: (id: number) => id === 2,
};
const ui = { openTrackMenu: vi.fn(), showDetails: vi.fn() };

vi.mock("../state/LibraryContext", () => ({ useLibrary: () => library }));
vi.mock("../state/UiContext", () => ({ useUi: () => ui }));

const track = (id: number, title: string, over: Partial<Track> = {}): Track => ({
  id,
  title,
  artist: "Artist",
  album: "Album",
  albumArtist: "Artist",
  genre: null,
  year: null,
  trackNo: null,
  durationMs: 180_000,
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

const a = track(1, "Harbor Lights");
const b = track(2, "Paper Kites");
const c = track(3, "Night Bus");
const dataTransfer = { setData: vi.fn(), effectAllowed: "", dropEffect: "" };
const bodyRows = () => screen.getAllByRole("row").slice(1);

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe("TrackTable", () => {
  it("shows each track with its details and marks favorites", () => {
    const { rows, playList } = buildRows([a, b, c], "", DEFAULT_SORT);
    render(<TrackTable rows={rows} playList={playList} label="Songs" />);
    expect(screen.getByRole("table", { name: "Songs" })).toBeTruthy();
    expect(bodyRows()).toHaveLength(3);
    expect(bodyRows()[0].textContent).toContain("Harbor Lights");
    expect(bodyRows()[0].textContent).toContain("FLAC · 44.1 kHz / 16-bit");
    expect(screen.getByRole("button", { name: "Remove Paper Kites from Favorites" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add Night Bus to Favorites" })).toBeTruthy();
  });

  it("opens details on click and plays on double-click or the row's play button", () => {
    const { rows, playList } = buildRows([a, b, c], "", DEFAULT_SORT);
    render(<TrackTable rows={rows} playList={playList} label="Songs" />);
    fireEvent.click(bodyRows()[1]);
    expect(ui.showDetails).toHaveBeenCalledWith(b, expect.objectContaining({ index: 1 }));
    expect(library.playTracks).not.toHaveBeenCalled();

    fireEvent.doubleClick(bodyRows()[2]);
    expect(library.playTracks).toHaveBeenLastCalledWith(playList, 2);
    fireEvent.click(screen.getByRole("button", { name: "Play Harbor Lights" }));
    expect(library.playTracks).toHaveBeenLastCalledWith(playList, 0);
  });

  // Regression (audit defect 1): the second copy of a track in a filtered
  // playlist must act on its own entry, not on the first copy.
  it("keeps the identity of a duplicate entry when the list is filtered", () => {
    const playlist = [a, b, a, c];
    const { rows, playList } = buildRows(playlist, "harbor", DEFAULT_SORT);
    render(
      <TrackTable rows={rows} playList={playList} label="Mix" playlistId="playlist:1" entryCount={4} />,
    );
    expect(bodyRows()).toHaveLength(2);
    fireEvent.contextMenu(bodyRows()[1]);
    const context = ui.openTrackMenu.mock.calls[0][1];
    expect(context.entryIndex).toBe(2);
    expect(context.index).toBe(2);
    expect(context.playlistId).toBe("playlist:1");

    fireEvent.doubleClick(bodyRows()[1]);
    expect(library.playTracks).toHaveBeenLastCalledWith(playList, 2);
  });

  it("can be used with the keyboard alone", () => {
    const { rows, playList } = buildRows([a, b, c], "", DEFAULT_SORT);
    render(<TrackTable rows={rows} playList={playList} label="Songs" />);
    const [first, second] = bodyRows();
    expect(first.getAttribute("tabindex")).toBe("0");
    expect(second.getAttribute("tabindex")).toBe("-1");

    first.focus();
    fireEvent.keyDown(first, { key: "ArrowDown" });
    expect(document.activeElement).toBe(bodyRows()[1]);
    fireEvent.keyDown(bodyRows()[1], { key: "End" });
    expect(document.activeElement).toBe(bodyRows()[2]);

    fireEvent.keyDown(bodyRows()[2], { key: "Enter" });
    expect(ui.showDetails).toHaveBeenCalledWith(c, expect.anything());
    fireEvent.keyDown(bodyRows()[2], { key: " " });
    expect(library.playTracks).toHaveBeenCalledWith(playList, 2);
    fireEvent.keyDown(bodyRows()[2], { key: "ContextMenu" });
    fireEvent.keyDown(bodyRows()[2], { key: "F10", shiftKey: true });
    expect(ui.openTrackMenu).toHaveBeenCalledTimes(2);
    expect(ui.openTrackMenu.mock.calls[0][0]).toBe(c);
  });

  it("does not play a track whose file is missing", () => {
    const missing = track(9, "Gone", { available: false });
    const { rows, playList } = buildRows([missing], "", DEFAULT_SORT);
    render(<TrackTable rows={rows} playList={playList} label="Songs" />);
    expect(bodyRows()[0].textContent).toContain("File missing");
    fireEvent.doubleClick(bodyRows()[0]);
    fireEvent.keyDown(bodyRows()[0], { key: " " });
    expect(library.playTracks).not.toHaveBeenCalled();
    expect((screen.getByRole("button", { name: "Play Gone" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("reorders by drag and drop only when reordering is allowed", () => {
    const onMove = vi.fn();
    const { rows, playList } = buildRows([a, b, c], "", DEFAULT_SORT);
    const view = render(<TrackTable rows={rows} playList={playList} label="Mix" onMove={onMove} />);
    expect(bodyRows()[0].getAttribute("draggable")).toBe("true");
    fireEvent.dragStart(bodyRows()[0], { dataTransfer });
    fireEvent.dragOver(bodyRows()[2], { dataTransfer });
    fireEvent.drop(bodyRows()[2], { dataTransfer });
    expect(onMove).toHaveBeenCalledWith(0, 2);

    view.rerender(<TrackTable rows={rows} playList={playList} label="Mix" />);
    expect(bodyRows()[0].getAttribute("draggable")).toBe("false");
  });

  it("sorts from the column headers and reports the active order", () => {
    const onSort = vi.fn();
    const { rows, playList } = buildRows([a, b, c], "", { field: "title", descending: true });
    render(
      <TrackTable
        rows={rows}
        playList={playList}
        label="Songs"
        sort={{ field: "title", descending: true }}
        onSort={onSort}
      />,
    );
    const header = screen.getByRole("columnheader", { name: "Title" });
    expect(header.getAttribute("aria-sort")).toBe("descending");
    fireEvent.click(screen.getByRole("button", { name: "Album" }));
    expect(onSort).toHaveBeenCalledWith("album");
    fireEvent.click(screen.getByRole("button", { name: "Default order" }));
    expect(onSort).toHaveBeenCalledWith("default");
  });

  it("toggles a favorite without opening details", () => {
    const { rows, playList } = buildRows([a, b], "", DEFAULT_SORT);
    render(<TrackTable rows={rows} playList={playList} label="Songs" />);
    fireEvent.click(screen.getByRole("button", { name: "Add Harbor Lights to Favorites" }));
    expect(library.setFavorite).toHaveBeenCalledWith(a, true);
    expect(ui.showDetails).not.toHaveBeenCalled();
  });

  it("renders only the rows near the viewport for a long list", () => {
    const many = Array.from({ length: 2000 }, (_, i) => track(i + 1, `Track ${i + 1}`));
    const { rows, playList } = buildRows(many, "", DEFAULT_SORT);
    render(<TrackTable rows={rows} playList={playList} label="Songs" />);
    const rendered = bodyRows().length;
    expect(rendered).toBeGreaterThan(0);
    expect(rendered).toBeLessThan(VIRTUAL_THRESHOLD);
    expect(screen.getByRole("table").getAttribute("aria-rowcount")).toBe("2001");
    // Rows that are not rendered still take up their space.
    const body = screen.getByRole("rowgroup");
    expect(parseFloat(body.style.paddingBottom)).toBeGreaterThan(50_000);
    expect(bodyRows()[0].getAttribute("aria-rowindex")).toBe("2");
  });
});
