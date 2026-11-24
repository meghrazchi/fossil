import { describe, expect, it } from 'vitest';
import { decodeTreemapBinary } from './ipc';

function makeSampleRectBytes(): Uint8Array {
  const buf = new ArrayBuffer(32);
  const view = new DataView(buf);
  view.setFloat32(0, 10.5, true);
  view.setFloat32(4, 20.5, true);
  view.setFloat32(8, 300.0, true);
  view.setFloat32(12, 150.0, true);
  view.setUint32(16, 42, true);
  view.setUint32(20, 1024, true);
  view.setUint32(24, 0, true);
  view.setUint8(28, 3);
  view.setUint8(29, 1);
  view.setUint8(30, 1);
  view.setUint8(31, 8);
  return new Uint8Array(buf);
}

describe('decodeTreemapBinary', () => {
  it('decodes ArrayBuffer, Uint8Array, and number[] identically', () => {
    const u8 = makeSampleRectBytes();

    for (const input of [u8.buffer, u8, Array.from(u8)]) {
      const rects = decodeTreemapBinary(input);
      expect(rects).toHaveLength(1);
      expect(rects[0]).toEqual({
        x: 10.5,
        y: 20.5,
        w: 300,
        h: 150,
        nodeId: 42,
        size: 1024,
        categoryIndex: 3,
        depth: 1,
        kindIndex: 1,
        flags: 8,
      });
    }
  });

  it('returns empty array for empty or malformed payloads', () => {
    expect(decodeTreemapBinary(null)).toEqual([]);
    expect(decodeTreemapBinary(new ArrayBuffer(0))).toEqual([]);
    expect(decodeTreemapBinary([])).toEqual([]);
  });
});
