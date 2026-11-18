import { describe, expect, it } from "vitest";
import {
  canGoBack,
  canGoForward,
  currentRoute,
  goBack,
  goForward,
  initialNav,
  navigate,
  type NavState,
} from "./router";

describe("router", () => {
  it("starts on Home with no history", () => {
    expect(currentRoute(initialNav)).toEqual({ page: "home" });
    expect(canGoBack(initialNav)).toBe(false);
    expect(canGoForward(initialNav)).toBe(false);
    expect(goBack(initialNav)).toBe(initialNav);
    expect(goForward(initialNav)).toBe(initialNav);
  });

  it("navigates, goes back, and goes forward", () => {
    let nav = navigate(initialNav, { page: "explore" });
    nav = navigate(nav, { page: "settings" });
    expect(currentRoute(nav).page).toBe("settings");
    nav = goBack(nav);
    expect(currentRoute(nav).page).toBe("explore");
    expect(canGoForward(nav)).toBe(true);
    nav = goForward(nav);
    expect(currentRoute(nav).page).toBe("settings");
  });

  it("ignores navigation to the current route", () => {
    const nav = navigate(initialNav, { page: "library", filter: "albums" });
    expect(navigate(nav, { page: "library", filter: "albums" })).toBe(nav);
    expect(navigate(nav, { page: "library", filter: "artists" })).not.toBe(nav);
  });

  it("drops forward history when navigating after going back", () => {
    let nav = navigate(initialNav, { page: "explore" });
    nav = navigate(nav, { page: "settings" });
    nav = goBack(goBack(nav));
    nav = navigate(nav, { page: "favorites" });
    expect(nav.stack.map((r) => r.page)).toEqual(["home", "favorites"]);
    expect(canGoForward(nav)).toBe(false);
  });

  it("caps history length", () => {
    let nav: NavState = initialNav;
    for (let i = 0; i < 120; i++) {
      nav = navigate(nav, i % 2 ? { page: "explore" } : { page: "search" });
    }
    expect(nav.stack.length).toBe(50);
    expect(nav.index).toBe(49);
  });
});

describe("collection routes", () => {
  it("treats different collections as different routes", () => {
    const nav = navigate(initialNav, { page: "collection", id: "a" });
    expect(navigate(nav, { page: "collection", id: "a" })).toBe(nav);
    const next = navigate(nav, { page: "collection", id: "b" });
    expect(currentRoute(next)).toEqual({ page: "collection", id: "b" });
    expect(currentRoute(goBack(next))).toEqual({ page: "collection", id: "a" });
  });
});

import { navStartingAt, parseRoute, serializeRoute, canGoBack as canBack } from "./router";

describe("saved routes", () => {
  it("round-trips every kind of route", () => {
    const routes = [
      { page: "home" },
      { page: "settings" },
      { page: "library", filter: "recent" },
      { page: "collection", id: "album:Mira\u001fLow Tide" },
    ] as const;
    for (const route of routes) {
      expect(parseRoute(serializeRoute(route))).toEqual(route);
    }
  });

  it("rejects anything that is not a known route", () => {
    for (const bad of ["", "{", "null", "[]", '{"page":"admin"}', '{"page":"library","filter":"x"}', '{"page":"collection"}', '{"page":"collection","id":""}']) {
      expect(parseRoute(bad)).toBeNull();
    }
  });

  it("restores the last page with Home behind it", () => {
    const nav = navStartingAt({ page: "favorites" });
    expect(currentRoute(nav)).toEqual({ page: "favorites" });
    expect(canBack(nav)).toBe(true);
    expect(navStartingAt(null)).toBe(initialNav);
    expect(navStartingAt({ page: "home" })).toBe(initialNav);
  });
});
