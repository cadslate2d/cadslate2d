import { describe, expect, test, vi } from "vitest";
import type { UiState } from "../cad/engine";
import { LOG_LINES_KEPT, UiStore, type UiSource } from "./ui-store";

class FakeSource implements UiSource {
  lines: string[] = [];
  state: UiState = {
    prompt: "Command:",
    logTotal: 0,
    selectionCount: 0,
    spaceAccepts: true,
    canUndo: false,
    canRedo: false,
    toggles: 0,
    unsavedChanges: false,
    exportIsLossy: false,
    layers: [],
  };
  logReads = 0;

  write(...lines: string[]): void {
    this.lines.push(...lines);
    this.state = { ...this.state, logTotal: this.lines.length };
  }

  uiState(): UiState {
    return this.state;
  }

  logSince(since: number): string[] {
    this.logReads += 1;
    return this.lines.slice(since);
  }
}

describe("UiStore", () => {
  test("starts at the idle prompt with an empty log", () => {
    expect(new UiStore().getSnapshot()).toMatchObject({
      prompt: "Command:",
      log: [],
      cursor: undefined,
    });
  });

  test("refresh mirrors engine state and notifies subscribers", () => {
    const [store, source] = [new UiStore(), new FakeSource()];
    const listener = vi.fn();
    store.subscribe(listener);

    source.state = {
      ...source.state,
      prompt: "Specify first point:",
      command: "LINE",
      cursor: [1, 2],
    };
    store.refresh(source);

    expect(store.getSnapshot()).toMatchObject({
      prompt: "Specify first point:",
      command: "LINE",
      cursor: [1, 2],
    });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  test("appends only new log lines", () => {
    const [store, source] = [new UiStore(), new FakeSource()];
    source.write("Command: L");
    store.refresh(source);
    source.write("Specify first point: 0,0", "Specify next point:");
    store.refresh(source);

    expect(store.getSnapshot().log).toEqual([
      "Command: L",
      "Specify first point: 0,0",
      "Specify next point:",
    ]);
  });

  test("does not read the log when nothing was written", () => {
    const [store, source] = [new UiStore(), new FakeSource()];
    source.write("a");
    store.refresh(source);
    store.refresh(source);
    store.refresh(source);
    expect(source.logReads).toBe(1);
  });

  test("keeps the same log array when only the cursor moved", () => {
    const [store, source] = [new UiStore(), new FakeSource()];
    source.write("a");
    store.refresh(source);
    const log = store.getSnapshot().log;
    source.state = { ...source.state, cursor: [5, 5] };
    store.refresh(source);
    expect(store.getSnapshot().log).toBe(log);
  });

  test("bounds the displayed log", () => {
    const [store, source] = [new UiStore(), new FakeSource()];
    source.write(...Array.from({ length: LOG_LINES_KEPT + 50 }, (_, index) => `line ${index}`));
    store.refresh(source);
    const { log } = store.getSnapshot();
    expect(log).toHaveLength(LOG_LINES_KEPT);
    expect(log.at(-1)).toBe(`line ${LOG_LINES_KEPT + 49}`);
  });

  test("unsubscribed listeners are not called", () => {
    const [store, source] = [new UiStore(), new FakeSource()];
    const listener = vi.fn();
    store.subscribe(listener)();
    store.refresh(source);
    expect(listener).not.toHaveBeenCalled();
  });
});
