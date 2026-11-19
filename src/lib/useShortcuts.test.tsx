// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ShortcutAction } from "./shortcuts";
import { useShortcuts, type ShortcutHandlers } from "./useShortcuts";

const calls: ShortcutAction[] = [];
const handlers = new Proxy({} as ShortcutHandlers, {
  get: (_, action: ShortcutAction) => () => calls.push(action),
});

function Harness({ children }: { children?: React.ReactNode }) {
  useShortcuts(handlers);
  return (
    <div>
      <input aria-label="Search your music" />
      <button>A button</button>
      <div role="row" tabIndex={0}>
        A row
      </div>
      {children}
    </div>
  );
}

beforeEach(() => {
  calls.length = 0;
});
afterEach(cleanup);

describe("useShortcuts", () => {
  it("runs the matching action and stops the browser default", () => {
    render(<Harness />);
    const space = new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true });
    document.body.dispatchEvent(space);
    expect(calls).toEqual(["togglePlay"]);
    expect(space.defaultPrevented).toBe(true);

    fireEvent.keyDown(document.body, { key: "ArrowRight", ctrlKey: true });
    fireEvent.keyDown(document.body, { key: "ArrowUp", ctrlKey: true, repeat: true });
    fireEvent.keyDown(document.body, { key: "m", ctrlKey: true });
    fireEvent.keyDown(document.body, { key: "F1" });
    expect(calls).toEqual(["togglePlay", "next", "volumeUp", "mute", "help"]);
  });

  it("leaves typing alone but still offers search", () => {
    render(<Harness />);
    const input = screen.getByLabelText("Search your music");
    const space = new KeyboardEvent("keydown", { key: " ", bubbles: true, cancelable: true });
    input.dispatchEvent(space);
    fireEvent.keyDown(input, { key: "ArrowLeft", ctrlKey: true });
    expect(calls).toEqual([]);
    expect(space.defaultPrevented).toBe(false);
    fireEvent.keyDown(input, { key: "f", ctrlKey: true });
    expect(calls).toEqual(["search"]);
  });

  it("lets Space activate a focused button or row", () => {
    render(<Harness />);
    fireEvent.keyDown(screen.getByText("A button"), { key: " " });
    fireEvent.keyDown(screen.getByText("A row"), { key: " " });
    expect(calls).toEqual([]);
    fireEvent.keyDown(screen.getByText("A row"), { key: "ArrowRight", ctrlKey: true });
    expect(calls).toEqual(["next"]);
  });

  it("does nothing while a dialog or menu is open", () => {
    render(
      <Harness>
        <dialog open>A dialog</dialog>
      </Harness>,
    );
    fireEvent.keyDown(document.body, { key: " " });
    fireEvent.keyDown(document.body, { key: "f", ctrlKey: true });
    expect(calls).toEqual([]);
  });

  it("skips keys another handler already dealt with", () => {
    render(<Harness />);
    const row = screen.getByText("A row");
    row.addEventListener("keydown", (event) => event.preventDefault());
    fireEvent.keyDown(row, { key: "ArrowRight", ctrlKey: true });
    expect(calls).toEqual([]);
  });

  it("stops listening when unmounted", () => {
    const view = render(<Harness />);
    view.unmount();
    fireEvent.keyDown(document.body, { key: " " });
    expect(calls).toEqual([]);
    vi.restoreAllMocks();
  });
});
