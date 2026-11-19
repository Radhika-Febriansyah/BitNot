// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Bootstrap } from "../lib/types";
import { AppProvider, useApp } from "./AppContext";

vi.mock("../lib/api", () => ({
  api: { updateSettings: vi.fn(() => Promise.resolve({})) },
  errorMessage: String,
}));

import { api } from "../lib/api";

const boot = {
  setupComplete: true,
  activeProfileId: "p1",
  profiles: [{ id: "p1", name: "Main", isMain: true, createdAt: 0 }],
  settings: { lastPage: JSON.stringify({ page: "nowPlaying" }) },
} as unknown as Bootstrap;

let app!: ReturnType<typeof useApp>;
function Probe() {
  app = useApp();
  return null;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const savedPages = () =>
  vi.mocked(api.updateSettings).mock.calls.map(([patch]) => (patch as { lastPage: string }).lastPage);

describe("AppProvider last page", () => {
  it("opens on the saved page and saves a return to it", () => {
    render(
      <AppProvider boot={boot} onBoot={() => undefined}>
        <Probe />
      </AppProvider>,
    );
    expect(app.route).toEqual({ page: "nowPlaying" });
    act(() => vi.advanceTimersByTime(1000));
    expect(api.updateSettings).not.toHaveBeenCalled();

    act(() => app.go({ page: "collection", id: "artist:Mira" }));
    act(() => vi.advanceTimersByTime(1000));
    act(() => app.go({ page: "nowPlaying" }));
    act(() => vi.advanceTimersByTime(1000));
    expect(savedPages()).toEqual([
      JSON.stringify({ page: "collection", id: "artist:Mira" }),
      JSON.stringify({ page: "nowPlaying" }),
    ]);
  });

  it("writes only once navigation settles", () => {
    render(
      <AppProvider boot={boot} onBoot={() => undefined}>
        <Probe />
      </AppProvider>,
    );
    act(() => app.go({ page: "settings" }));
    act(() => vi.advanceTimersByTime(200));
    act(() => app.go({ page: "favorites" }));
    act(() => vi.advanceTimersByTime(1000));
    expect(savedPages()).toEqual([JSON.stringify({ page: "favorites" })]);
  });
});
