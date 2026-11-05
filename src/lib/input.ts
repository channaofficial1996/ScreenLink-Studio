// Pure mapping from browser input to Android input, kept free of DOM state so it is unit tested.

export const KEY_DOWN = 0;
export const KEY_UP = 1;

export const MOTION_DOWN = 0;
export const MOTION_UP = 1;
export const MOTION_MOVE = 2;

const META_SHIFT = 0x41;
const META_ALT = 0x12;
const META_CTRL = 0x3000;
const META_LOGO = 0x30000;

export const KEYCODE_HOME = 3;
export const KEYCODE_BACK = 4;
export const KEYCODE_VOLUME_UP = 24;
export const KEYCODE_VOLUME_DOWN = 25;
export const KEYCODE_POWER = 26;
export const KEYCODE_APP_SWITCH = 187;

const NAMED_KEYS: Record<string, number> = {
  Enter: 66,
  NumpadEnter: 160,
  Backspace: 67,
  Delete: 112,
  Tab: 61,
  Escape: 111,
  ArrowUp: 19,
  ArrowDown: 20,
  ArrowLeft: 21,
  ArrowRight: 22,
  Home: 122,
  End: 123,
  PageUp: 92,
  PageDown: 93,
  Insert: 124,
  ShiftLeft: 59,
  ShiftRight: 60,
  AltLeft: 57,
  AltRight: 58,
  ControlLeft: 113,
  ControlRight: 114,
  MetaLeft: 117,
  MetaRight: 118,
  Space: 62,
  Minus: 69,
  Equal: 70,
  BracketLeft: 71,
  BracketRight: 72,
  Backslash: 73,
  Semicolon: 74,
  Quote: 75,
  Backquote: 68,
  Comma: 55,
  Period: 56,
  Slash: 76,
};

export function keycodeFor(code: string): number | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return 29 + letter[1].charCodeAt(0) - 65;
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return 7 + Number(digit[1]);
  const numpad = /^Numpad([0-9])$/.exec(code);
  if (numpad) return 144 + Number(numpad[1]);
  const fn = /^F([1-9]|1[0-2])$/.exec(code);
  if (fn) return 130 + Number(fn[1]);
  return NAMED_KEYS[code] ?? null;
}

export interface Modifiers {
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}

export function metaState(m: Modifiers): number {
  return (m.shiftKey ? META_SHIFT : 0) | (m.altKey ? META_ALT : 0) | (m.ctrlKey ? META_CTRL : 0) | (m.metaKey ? META_LOGO : 0);
}

/**
 * Plain typed characters go through text injection so every keyboard layout produces the right
 * character. Shortcuts and non-printing keys go through keycodes.
 */
export function isTextInput(e: { key: string } & Modifiers): boolean {
  return e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey;
}

/** Convert a point in the element to video coordinates. Null when it falls outside the video. */
export function toVideoPoint(
  clientX: number,
  clientY: number,
  rect: { left: number; top: number; width: number; height: number },
  videoWidth: number,
  videoHeight: number,
): { x: number; y: number } | null {
  if (rect.width === 0 || rect.height === 0) return null;
  const x = Math.round(((clientX - rect.left) / rect.width) * videoWidth);
  const y = Math.round(((clientY - rect.top) / rect.height) * videoHeight);
  if (x < 0 || y < 0 || x > videoWidth || y > videoHeight) return null;
  return { x: Math.min(x, videoWidth - 1), y: Math.min(y, videoHeight - 1) };
}

export function wheelToScroll(deltaX: number, deltaY: number, deltaMode: number): { hscroll: number; vscroll: number } {
  const unit = deltaMode === 1 ? 1 / 3 : deltaMode === 2 ? 3 : 1 / 100; // lines, pages, pixels
  const clamp = (v: number) => Math.max(-16, Math.min(16, v));
  return { hscroll: clamp(-deltaX * unit), vscroll: clamp(-deltaY * unit) };
}
