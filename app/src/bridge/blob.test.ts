import { describe, expect, it } from 'vitest';

import { decodeBlob, encodeBlob, heightsOf } from './blob';
import { floatHeights, HEIGHT_NAN, heightReader } from './heights';
import { KernelError, type BlobHeader, type HeightCodeHeader } from './types';

describe('blob framing', () => {
  it('round trips typed sections with their header fields', () => {
    const body = encodeBlob({ revision: 7, note: 'x' }, [
      { name: 'xy', data: Float32Array.from([1.5, -2, 3]) },
      { name: 'ids', data: Uint32Array.from([9, 10]) },
      { name: 'flags', data: Uint8Array.from([1, 0, 1]) },
    ]);
    const reply = decodeBlob<{ sections: never[]; revision: number; note: string }>(body, 'test');
    expect(reply.header.revision).toBe(7);
    expect(reply.header.note).toBe('x');
    expect(Array.from(reply.sections['xy'] ?? [])).toEqual([1.5, -2, 3]);
    expect(Array.from(reply.sections['ids'] ?? [])).toEqual([9, 10]);
    expect(Array.from(reply.sections['flags'] ?? [])).toEqual([1, 0, 1]);
    expect(reply.sections['xy']).toBeInstanceOf(Float32Array);
  });

  it('keeps sections aligned when an odd sized section precedes a float one', () => {
    const body = encodeBlob({}, [
      { name: 'flags', data: Uint8Array.from([1, 2, 3]) },
      { name: 'xy', data: Float32Array.from([4, 5]) },
    ]);
    const reply = decodeBlob(body, 'test');
    expect(Array.from(reply.sections['xy'] ?? [])).toEqual([4, 5]);
  });

  it('rejects a truncated reply', () => {
    const body = encodeBlob({}, [{ name: 'xy', data: Float32Array.from([1, 2, 3, 4]) }]);
    expect(() => decodeBlob(body.subarray(0, body.length - 4), 'test')).toThrow(KernelError);
  });
});

describe('height sections', () => {
  const reply = (header: object, z: Float32Array | Uint16Array) =>
    decodeBlob<BlobHeader & HeightCodeHeader>(
      encodeBlob<BlobHeader>(header, [{ name: 'z', data: z }]),
      'test',
    );

  it('keeps u16 steps as codes under the header base and step', () => {
    const out = heightsOf(
      reply({ heightBase: -2, heightStep: 0.5 }, Uint16Array.from([0, 3, HEIGHT_NAN])),
      'z',
    );
    expect(out?.data).toBeInstanceOf(Uint16Array);
    expect(out?.code).toEqual({ base: -2, step: 0.5 });
    const read = heightReader(out ?? floatHeights(new Float32Array(0)));
    expect(read(0)).toBe(-2);
    expect(read(1)).toBe(-0.5);
    expect(read(2)).toBeNaN();
  });

  it('passes f32 heights through and gives nothing for a missing section', () => {
    const r = reply({}, Float32Array.from([1.25, Number.NaN]));
    const z = heightsOf(r, 'z');
    expect(z?.code).toBeNull();
    expect(Array.from(z?.data ?? [])).toEqual([1.25, Number.NaN]);
    expect(heightsOf(r, 'floor')).toBeNull();
  });
});
