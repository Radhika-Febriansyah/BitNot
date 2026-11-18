import { afterEach, describe, expect, it } from "vitest";
import { currentLanguage, registerLanguage, setLanguage, t, tn } from "./index";

afterEach(() => setLanguage("en"));

describe("translations", () => {
  it("returns English messages and fills placeholders", () => {
    expect(t("nav.home")).toBe("Home");
    expect(t("table.play", { title: "Night Bus" })).toBe("Play Night Bus");
    expect(t("table.play")).toBe("Play {title}");
  });

  it("chooses plural forms", () => {
    expect(tn("count.songs", 1)).toBe("1 song");
    expect(tn("count.songs", 0)).toBe("0 songs");
    expect(tn("count.songs", 40)).toBe("40 songs");
  });

  it("uses another language where translated and English elsewhere", () => {
    registerLanguage("xx", {
      nav: { home: "Beranda" },
      count: { songs: { one: "{count} lagu", other: "{count} lagu" } },
    });
    setLanguage("xx");
    expect(currentLanguage()).toBe("xx");
    expect(t("nav.home")).toBe("Beranda");
    expect(t("nav.search")).toBe("Search");
    expect(tn("count.songs", 3)).toBe("3 lagu");
    expect(tn("count.files", 2)).toBe("2 files");
  });

  it("falls back to English for an unknown language", () => {
    setLanguage("zz");
    expect(currentLanguage()).toBe("en");
    expect(t("nav.home")).toBe("Home");
  });
});
