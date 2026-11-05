// Binary reply framing shared by the transports and the mock.
//
// Layout after the four tag bytes: a little endian u32 header length, the
// header as UTF-8 JSON, then the payload the header's sections index into.
// Large height sections may come as u16 steps; heightsOf keeps them so.

import {
  KernelError,
  type BlobHeader,
  type BlobReply,
  type HeightCodeHeader,
  type TypedArray,
} from './types';
import { codedHeights, floatHeights, type Heights } from './heights';

const decoder = new TextDecoder();
const encoder = new TextEncoder();

function viewFor(
  dtype: string,
  buffer: ArrayBuffer,
  offset: number,
  count: number,
  command: string,
): TypedArray {
  switch (dtype) {
    case 'f32':
      return new Float32Array(buffer, offset, count);
    case 'u32':
      return new Uint32Array(buffer, offset, count);
    case 'u16':
      return new Uint16Array(buffer, offset, count);
    case 'u8':
      return new Uint8Array(buffer, offset, count);
    default:
      throw new KernelError(command, `unknown section type ${dtype} in a binary reply`);
  }
}

/** Decodes the bytes that follow the `BLOB` tag. */
export function decodeBlob<H extends BlobHeader>(body: Uint8Array, command: string): BlobReply<H> {
  if (body.length < 4) {
    throw new KernelError(command, 'the kernel answered with a truncated binary reply');
  }
  const headerLength = new DataView(body.buffer, body.byteOffset, body.byteLength).getUint32(
    0,
    true,
  );
  const headerEnd = 4 + headerLength;
  if (headerEnd > body.length) {
    throw new KernelError(command, 'the binary reply header runs past the end of the reply');
  }
  const header = JSON.parse(decoder.decode(body.subarray(4, headerEnd))) as H;
  const payload = body.byteLength - headerEnd;
  const sections: Record<string, TypedArray> = {};
  for (const section of header.sections) {
    const bytesPer = section.dtype === 'u8' ? 1 : section.dtype === 'u16' ? 2 : 4;
    const end = section.offset + section.count * bytesPer;
    if (end > payload) {
      throw new KernelError(command, `section ${section.name} runs past the end of the reply`);
    }
    // One copy per section, so each starts on an aligned buffer of its own.
    const start = headerEnd + section.offset;
    const aligned = body.slice(start, start + section.count * bytesPer);
    sections[section.name] = viewFor(section.dtype, aligned.buffer, 0, section.count, command);
  }
  return { header, sections };
}

/**
 * Heights from the section `name` as sent: `f32`, or `u16` steps where code
 * `q` is `heightBase + q * heightStep` mm (see bridge/heights.ts). The codes
 * are kept, never copied out into floats. Null when the section is missing.
 */
export function heightsOf(
  reply: BlobReply<BlobHeader & HeightCodeHeader>,
  name: string,
): Heights | null {
  const section = reply.sections[name];
  if (section instanceof Float32Array) {
    return floatHeights(section);
  }
  if (!(section instanceof Uint16Array)) {
    return null;
  }
  return codedHeights(section, {
    base: reply.header.heightBase ?? 0,
    step: reply.header.heightStep ?? 0,
  });
}

/** Encodes a header and typed sections into the body that follows the tag. */
export function encodeBlob<H extends BlobHeader>(
  header: Omit<H, 'sections'>,
  parts: ReadonlyArray<{ name: string; data: TypedArray }>,
): Uint8Array {
  const sections = [];
  let offset = 0;
  for (const part of parts) {
    const dtype =
      part.data instanceof Float32Array
        ? 'f32'
        : part.data instanceof Uint32Array
          ? 'u32'
          : part.data instanceof Uint16Array
            ? 'u16'
            : 'u8';
    sections.push({ name: part.name, dtype, offset, count: part.data.length });
    offset += part.data.byteLength;
    offset = (offset + 3) & ~3;
  }
  const headerBytes = encoder.encode(JSON.stringify({ ...header, sections }));
  const body = new Uint8Array(4 + headerBytes.length + offset);
  new DataView(body.buffer).setUint32(0, headerBytes.length, true);
  body.set(headerBytes, 4);
  const payloadStart = 4 + headerBytes.length;
  for (const [index, part] of parts.entries()) {
    const bytes = new Uint8Array(part.data.buffer, part.data.byteOffset, part.data.byteLength);
    body.set(bytes, payloadStart + sections[index].offset);
  }
  return body;
}
