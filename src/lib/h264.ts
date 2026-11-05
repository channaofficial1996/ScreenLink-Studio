/** Build the WebCodecs codec string (avc1.PPCCLL) from an Annex B config packet. */
export function codecFromConfig(config: Uint8Array): string | null {
  for (let i = 0; i + 7 < config.length; i++) {
    const isStartCode = config[i] === 0 && config[i + 1] === 0 && config[i + 2] === 1;
    if (isStartCode && (config[i + 3] & 0x1f) === 7) {
      const hex = (b: number) => b.toString(16).padStart(2, "0");
      return `avc1.${hex(config[i + 4])}${hex(config[i + 5])}${hex(config[i + 6])}`;
    }
  }
  return null;
}

export function concat(a: Uint8Array, b: Uint8Array): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}
