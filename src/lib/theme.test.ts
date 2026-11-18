import { describe, expect, it } from "vitest";
import {
  ACCENT_PRESETS,
  accentInk,
  contrastRatio,
  isValidAccent,
  resolveMotion,
  resolveTheme,
} from "./theme";

describe("theme", () => {
  it("validates accent colours", () => {
    expect(isValidAccent("#36C5F0")).toBe(true);
    expect(isValidAccent("#36c5f0")).toBe(true);
    expect(isValidAccent("36C5F0")).toBe(false);
    expect(isValidAccent("#36C5F")).toBe(false);
    expect(isValidAccent("#GGGGGG")).toBe(false);
    expect(isValidAccent("red")).toBe(false);
  });

  it("computes WCAG contrast", () => {
    expect(contrastRatio("#000000", "#FFFFFF")).toBeCloseTo(21, 5);
    expect(contrastRatio("#36C5F0", "#36C5F0")).toBeCloseTo(1, 5);
  });

  it("picks readable text for the accent", () => {
    expect(accentInk("#36C5F0")).toBe("#0B111B");
    expect(accentInk("#FFFFFF")).toBe("#0B111B");
    expect(accentInk("#1B2A6B")).toBe("#FFFFFF");
  });

  it("keeps every preset readable with its ink at AA level", () => {
    for (const preset of ACCENT_PRESETS) {
      expect(contrastRatio(preset.value, accentInk(preset.value))).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("resolves theme and motion against system preferences", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveMotion("system", true)).toBe("reduce");
    expect(resolveMotion("system", false)).toBe("full");
    expect(resolveMotion("full", true)).toBe("full");
    expect(resolveMotion("reduce", false)).toBe("reduce");
  });
});

// @ts-expect-error Node type definitions are not installed; the test runner provides node:fs.
import { readFileSync } from "node:fs";

const css: string = readFileSync("src/styles/tokens.css", "utf8");

describe("theme token contrast", () => {
  // Reads the real stylesheet so a later colour change cannot quietly break it.
  const block = (selector: string) => {
    const start = css.indexOf(selector);
    return css.slice(start, css.indexOf("}", start));
  };
  const token = (theme: string, name: string) => {
    const match = new RegExp(`--${name}: *(#[0-9a-fA-F]{6})`).exec(block(theme));
    if (!match) throw new Error(`token ${name} not found in ${theme}`);
    return match[1];
  };

  it.each([':root[data-theme="dark"]', ':root[data-theme="light"]'])(
    "text colours meet WCAG AA on both surfaces in %s",
    (theme) => {
      for (const surface of ["bg", "bg-panel"]) {
        for (const text of ["text", "text-muted", "text-faint"]) {
          const ratio = contrastRatio(token(theme, text), token(theme, surface));
          expect(ratio, `${text} on ${surface}`).toBeGreaterThanOrEqual(4.5);
        }
      }
    },
  );
});
