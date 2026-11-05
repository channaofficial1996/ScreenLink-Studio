import { describe, expect, it } from "vitest";
import { codecFromConfig, concat } from "./h264";
import { formatBytes, formatDuration, basename } from "./format";
import { isTextInput, keycodeFor, metaState, toVideoPoint, wheelToScroll } from "./input";

const noMods = { shiftKey: false, altKey: false, ctrlKey: false, metaKey: false };

describe("keycodeFor", () => {
  it("maps letters, digits and function keys to Android keycodes", () => {
    expect(keycodeFor("KeyA")).toBe(29);
    expect(keycodeFor("KeyZ")).toBe(54);
    expect(keycodeFor("Digit0")).toBe(7);
    expect(keycodeFor("Digit9")).toBe(16);
    expect(keycodeFor("Numpad5")).toBe(149);
    expect(keycodeFor("F1")).toBe(131);
    expect(keycodeFor("F12")).toBe(142);
  });

  it("maps navigation keys and rejects unknown ones", () => {
    expect(keycodeFor("Enter")).toBe(66);
    expect(keycodeFor("Backspace")).toBe(67);
    expect(keycodeFor("ArrowUp")).toBe(19);
    expect(keycodeFor("AudioVolumeMute")).toBeNull();
  });
});

describe("metaState", () => {
  it("combines modifiers with Android flags", () => {
    expect(metaState(noMods)).toBe(0);
    expect(metaState({ ...noMods, shiftKey: true })).toBe(0x41);
    expect(metaState({ ...noMods, ctrlKey: true, altKey: true })).toBe(0x3012);
  });
});

describe("isTextInput", () => {
  it("sends typed characters as text and shortcuts as keys", () => {
    expect(isTextInput({ ...noMods, key: "é" })).toBe(true);
    expect(isTextInput({ ...noMods, key: "A", shiftKey: true })).toBe(true);
    expect(isTextInput({ ...noMods, key: "c", ctrlKey: true })).toBe(false);
    expect(isTextInput({ ...noMods, key: "Enter" })).toBe(false);
  });
});

describe("toVideoPoint", () => {
  const rect = { left: 100, top: 50, width: 360, height: 800 };

  it("scales display coordinates to video coordinates", () => {
    expect(toVideoPoint(280, 450, rect, 720, 1600)).toEqual({ x: 360, y: 800 });
    expect(toVideoPoint(100, 50, rect, 720, 1600)).toEqual({ x: 0, y: 0 });
  });

  it("keeps the far edge inside the video", () => {
    expect(toVideoPoint(460, 850, rect, 720, 1600)).toEqual({ x: 719, y: 1599 });
  });

  it("rejects points outside the video and empty rects", () => {
    expect(toVideoPoint(99, 60, rect, 720, 1600)).toBeNull();
    expect(toVideoPoint(200, 900, rect, 720, 1600)).toBeNull();
    expect(toVideoPoint(1, 1, { left: 0, top: 0, width: 0, height: 0 }, 720, 1600)).toBeNull();
  });
});

describe("wheelToScroll", () => {
  it("inverts pixel deltas and clamps to the protocol range", () => {
    expect(wheelToScroll(0, 100, 0)).toEqual({ hscroll: -0, vscroll: -1 });
    expect(wheelToScroll(0, -5000, 0).vscroll).toBe(16);
  });
});

describe("codecFromConfig", () => {
  it("reads profile, constraints and level from the SPS", () => {
    const config = new Uint8Array([0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1f, 0xda, 0, 0, 0, 1, 0x68, 0xce]);
    expect(codecFromConfig(config)).toBe("avc1.42c01f");
  });

  it("returns null without an SPS", () => {
    expect(codecFromConfig(new Uint8Array([0, 0, 0, 1, 0x68, 0xce, 0, 0, 0]))).toBeNull();
  });
});

describe("formatting", () => {
  it("concatenates byte arrays", () => {
    expect([...concat(new Uint8Array([1]), new Uint8Array([2, 3]))]).toEqual([1, 2, 3]);
  });

  it("formats sizes, durations and paths", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(50 * 1024 * 1024)).toBe("50 MB");
    expect(formatDuration(65)).toBe("01:05");
    expect(formatDuration(3725)).toBe("01:02:05");
    expect(basename("C:\\Videos\\a.mp4")).toBe("a.mp4");
    expect(basename("/Users/x/a.mp4")).toBe("a.mp4");
  });
});
