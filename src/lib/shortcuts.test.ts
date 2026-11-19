import { describe, expect, it } from "vitest";
import { shortcutFor, shortcutHint, SHORTCUTS, type KeyContext, type KeyInput } from "./shortcuts";

const key = (k: string, mods: Partial<KeyInput> = {}): KeyInput => ({
  key: k,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  repeat: false,
  ...mods,
});
const free: KeyContext = { typing: false, onControl: false, overlayOpen: false };
const typing: KeyContext = { ...free, typing: true, onControl: true };
const onButton: KeyContext = { ...free, onControl: true };
const overlay: KeyContext = { ...free, overlayOpen: true };

describe("shortcutFor", () => {
  it("maps the shortcuts from the specification", () => {
    expect(shortcutFor(key(" "), free)).toBe("togglePlay");
    expect(shortcutFor(key("ArrowRight", { ctrlKey: true }), free)).toBe("next");
    expect(shortcutFor(key("ArrowLeft", { ctrlKey: true }), free)).toBe("previous");
    expect(shortcutFor(key("ArrowUp", { ctrlKey: true }), free)).toBe("volumeUp");
    expect(shortcutFor(key("ArrowDown", { ctrlKey: true }), free)).toBe("volumeDown");
    expect(shortcutFor(key("f", { ctrlKey: true }), free)).toBe("search");
  });

  it("maps the additional shortcuts", () => {
    expect(shortcutFor(key("m", { ctrlKey: true }), free)).toBe("mute");
    expect(shortcutFor(key("M", { ctrlKey: true }), free)).toBe("mute");
    expect(shortcutFor(key("ArrowRight", { ctrlKey: true, shiftKey: true }), free)).toBe("seekForward");
    expect(shortcutFor(key("ArrowLeft", { ctrlKey: true, shiftKey: true }), free)).toBe("seekBack");
    expect(shortcutFor(key("F1"), free)).toBe("help");
    expect(shortcutFor(key("?", { shiftKey: true }), free)).toBe("help");
  });

  it("leaves keys alone while typing, except search and help", () => {
    expect(shortcutFor(key(" "), typing)).toBeNull();
    expect(shortcutFor(key("ArrowRight", { ctrlKey: true }), typing)).toBeNull();
    expect(shortcutFor(key("ArrowUp", { ctrlKey: true }), typing)).toBeNull();
    expect(shortcutFor(key("m", { ctrlKey: true }), typing)).toBeNull();
    expect(shortcutFor(key("?", { shiftKey: true }), typing)).toBeNull();
    expect(shortcutFor(key("f", { ctrlKey: true }), typing)).toBe("search");
    expect(shortcutFor(key("F1"), typing)).toBe("help");
  });

  it("lets Space activate a focused button or row instead of toggling playback", () => {
    expect(shortcutFor(key(" "), onButton)).toBeNull();
    expect(shortcutFor(key("ArrowRight", { ctrlKey: true }), onButton)).toBe("next");
  });

  it("does nothing while a dialog or menu is open", () => {
    for (const k of [key(" "), key("F1"), key("f", { ctrlKey: true }), key("ArrowUp", { ctrlKey: true })]) {
      expect(shortcutFor(k, overlay)).toBeNull();
    }
  });

  it("leaves Alt and Windows-key combinations to the system", () => {
    expect(shortcutFor(key("ArrowRight", { ctrlKey: true, altKey: true }), free)).toBeNull();
    expect(shortcutFor(key("ArrowRight", { metaKey: true }), free)).toBeNull();
    expect(shortcutFor(key(" ", { altKey: true }), free)).toBeNull();
  });

  it("ignores unrelated keys and other modifier mixes", () => {
    expect(shortcutFor(key("a"), free)).toBeNull();
    expect(shortcutFor(key("ArrowRight"), free)).toBeNull();
    expect(shortcutFor(key(" ", { shiftKey: true }), free)).toBeNull();
    expect(shortcutFor(key("ArrowUp", { ctrlKey: true, shiftKey: true }), free)).toBeNull();
  });

  it("repeats only volume and seek while a key is held", () => {
    expect(shortcutFor(key("ArrowUp", { ctrlKey: true, repeat: true }), free)).toBe("volumeUp");
    expect(shortcutFor(key("ArrowRight", { ctrlKey: true, shiftKey: true, repeat: true }), free)).toBe("seekForward");
    expect(shortcutFor(key(" ", { repeat: true }), free)).toBeNull();
    expect(shortcutFor(key("ArrowRight", { ctrlKey: true, repeat: true }), free)).toBeNull();
    expect(shortcutFor(key("m", { ctrlKey: true, repeat: true }), free)).toBeNull();
  });
});

describe("shortcut reference", () => {
  it("lists every action once and gives tooltip hints", () => {
    const actions = SHORTCUTS.map((s) => s.action);
    expect(new Set(actions).size).toBe(actions.length);
    expect(shortcutHint("next")).toBe("Ctrl+→");
    expect(shortcutHint("togglePlay")).toBe("Space");
    expect(shortcutHint("help")).toBe("F1");
  });
});
