// A small in-memory router with back/forward history, as in the top bar of
// the library reference. Pure functions so the behaviour is unit-tested.

export type LibraryFilter =
  | "all"
  | "playlists"
  | "albums"
  | "artists"
  | "local"
  | "recent"
  | "online";

const FILTERS: LibraryFilter[] = ["all", "playlists", "albums", "artists", "local", "recent", "online"];
const PLAIN_PAGES = ["home", "explore", "search", "favorites", "recent", "settings"] as const;

export type Route =
  | { page: "home" }
  | { page: "explore" }
  | { page: "search" }
  | { page: "library"; filter: LibraryFilter }
  | { page: "favorites" }
  | { page: "recent" }
  | { page: "collection"; id: string }
  | { page: "settings" };

export interface NavState {
  stack: Route[];
  index: number;
}

const MAX_HISTORY = 50;

export const initialNav: NavState = { stack: [{ page: "home" }], index: 0 };

export function sameRoute(a: Route, b: Route): boolean {
  if (a.page !== b.page) return false;
  if (a.page === "library" && b.page === "library") return a.filter === b.filter;
  if (a.page === "collection" && b.page === "collection") return a.id === b.id;
  return true;
}

/** Text form of a route, saved so the last page can be restored. */
export function serializeRoute(route: Route): string {
  return JSON.stringify(route);
}

/**
 * Reads a saved route. Anything that is not exactly a known route yields
 * null, so damaged or outdated settings simply open Home.
 */
export function parseRoute(text: string): Route | null {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  if (value === null || typeof value !== "object") return null;
  const { page, filter, id } = value as Record<string, unknown>;
  if (page === "library") {
    return FILTERS.includes(filter as LibraryFilter)
      ? { page, filter: filter as LibraryFilter }
      : null;
  }
  if (page === "collection") {
    return typeof id === "string" && id.length > 0 && id.length <= 500 ? { page, id } : null;
  }
  const plain = PLAIN_PAGES.find((name) => name === page);
  return plain ? { page: plain } : null;
}

/** History that starts on a restored page, with Home behind it. */
export function navStartingAt(route: Route | null): NavState {
  return route && route.page !== "home" ? navigate(initialNav, route) : initialNav;
}

export function currentRoute(state: NavState): Route {
  return state.stack[state.index];
}

export function navigate(state: NavState, to: Route): NavState {
  if (sameRoute(currentRoute(state), to)) return state;
  const stack = [...state.stack.slice(0, state.index + 1), to].slice(-MAX_HISTORY);
  return { stack, index: stack.length - 1 };
}

export const canGoBack = (state: NavState) => state.index > 0;
export const canGoForward = (state: NavState) => state.index < state.stack.length - 1;

export function goBack(state: NavState): NavState {
  return canGoBack(state) ? { ...state, index: state.index - 1 } : state;
}

export function goForward(state: NavState): NavState {
  return canGoForward(state) ? { ...state, index: state.index + 1 } : state;
}
