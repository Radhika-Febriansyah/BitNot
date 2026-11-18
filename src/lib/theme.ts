// Turns stored appearance settings into attributes and CSS variables on the
// document root. All component styles read those; nothing else sets colours.

import type { MotionPreference, Settings, Theme } from "./types";

export const BRAND_ACCENT = "#36C5F0";

export const ACCENT_PRESETS: { name: string; value: string }[] = [
  { name: "Cyan Blue", value: "#36C5F0" },
  { name: "Mint", value: "#3DDC97" },
  { name: "Violet", value: "#9B8CFF" },
  { name: "Rose", value: "#FF6B9A" },
  { name: "Amber", value: "#FFB347" },
  { name: "Coral", value: "#FF7A59" },
];

export const SCALE_OPTIONS = [90, 100, 110, 125, 150];

const INK_DARK = "#0B111B";
const INK_LIGHT = "#FFFFFF";

export function isValidAccent(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

function channel(value: number): number {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG relative luminance of a `#RRGGBB` colour. */
export function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  return (
    0.2126 * channel((n >> 16) & 255) +
    0.7152 * channel((n >> 8) & 255) +
    0.0722 * channel(n & 255)
  );
}

export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Text colour to place on top of the accent, whichever reads better. */
export function accentInk(accent: string): string {
  return contrastRatio(accent, INK_DARK) >= contrastRatio(accent, INK_LIGHT)
    ? INK_DARK
    : INK_LIGHT;
}

export function resolveTheme(preference: Theme, systemPrefersDark: boolean): "dark" | "light" {
  if (preference === "system") return systemPrefersDark ? "dark" : "light";
  return preference;
}

export function resolveMotion(
  preference: MotionPreference,
  systemPrefersReduced: boolean,
): "reduce" | "full" {
  if (preference === "system") return systemPrefersReduced ? "reduce" : "full";
  return preference;
}

export interface SystemPreferences {
  prefersDark: boolean;
  prefersReducedMotion: boolean;
}

export function applyAppearance(
  root: HTMLElement,
  settings: Pick<Settings, "theme" | "accentColor" | "displayScale" | "reduceMotion" | "language">,
  system: SystemPreferences,
): void {
  const accent = isValidAccent(settings.accentColor) ? settings.accentColor : BRAND_ACCENT;
  root.dataset.theme = resolveTheme(settings.theme, system.prefersDark);
  root.dataset.motion = resolveMotion(settings.reduceMotion, system.prefersReducedMotion);
  root.lang = settings.language;
  root.style.setProperty("--accent", accent);
  root.style.setProperty("--accent-ink", accentInk(accent));
  // Every size in the stylesheets is in rem, so this scales the whole interface.
  root.style.fontSize = `${settings.displayScale}%`;
}
