// Decodes framed kernel replies, shared by every transport that talks to the
// real kernel. Per the kernel API contract a reply is four ASCII tag bytes,
// `JSON` or `BLOB`, then the payload. A JSON payload is the reply envelope,
// either `{"ok": true, "result": ...}` or `{"ok": false, "error": {code,
// message}}`.

import { decodeBlob } from './blob';
import { KernelError, type BlobHeader, type BlobReply } from './types';

const TAG_LENGTH = 4;
const TAG_JSON = 'JSON';
const TAG_BLOB = 'BLOB';
const decoder = new TextDecoder();

export type RawReply = ArrayBuffer | Uint8Array | number[];

interface FailureEnvelope {
  ok: false;
  error: { code?: string; message?: string };
}

interface SuccessEnvelope {
  ok: true;
  result: unknown;
}

type Envelope = SuccessEnvelope | FailureEnvelope;

export interface Framed {
  tag: string;
  body: Uint8Array;
}

function toBytes(raw: RawReply): Uint8Array {
  if (raw instanceof Uint8Array) {
    return raw;
  }
  if (Array.isArray(raw)) {
    return Uint8Array.from(raw);
  }
  return new Uint8Array(raw);
}

/** Splits a framed reply into its tag and payload. */
export function splitFrame(name: string, raw: RawReply): Framed {
  const bytes = toBytes(raw);
  if (bytes.length < TAG_LENGTH) {
    throw new KernelError(name, 'the kernel answered with a truncated reply');
  }
  return { tag: decoder.decode(bytes.subarray(0, TAG_LENGTH)), body: bytes.subarray(TAG_LENGTH) };
}

/**
 * A kernel refusal as the user reads it: the kernel writes its messages in
 * plain words starting in lower case; the first word gets its capital. A
 * message that starts with a name (a file, a font) is left as it is.
 */
export function refusalText(message: string): string {
  return /^[a-z]+[\s,:;]/.test(message)
    ? message.charAt(0).toUpperCase() + message.slice(1)
    : message;
}

function unwrapJson<T>(name: string, body: Uint8Array): T {
  const envelope = JSON.parse(decoder.decode(body)) as Envelope;
  if (!envelope.ok) {
    throw new KernelError(
      name,
      refusalText(envelope.error.message ?? 'the kernel refused the command'),
      envelope.error.code ?? 'kernel',
    );
  }
  return envelope.result as T;
}

/** The result of a JSON reply, or the kernel's refusal as a KernelError. */
export function jsonResult<T>(name: string, framed: Framed): T {
  if (framed.tag !== TAG_JSON) {
    throw new KernelError(
      name,
      `the kernel answered with a ${framed.tag} reply where JSON was expected`,
    );
  }
  return unwrapJson<T>(name, framed.body);
}

/** The decoded binary reply, or the kernel's refusal as a KernelError. */
export function blobResult<H extends BlobHeader>(name: string, framed: Framed): BlobReply<H> {
  if (framed.tag === TAG_JSON) {
    // Errors come back as JSON even for binary commands.
    unwrapJson<unknown>(name, framed.body);
    throw new KernelError(name, 'the kernel answered with JSON where a binary reply was expected');
  }
  if (framed.tag !== TAG_BLOB) {
    throw new KernelError(name, `the kernel answered with an unknown reply tag ${framed.tag}`);
  }
  return decodeBlob<H>(framed.body, name);
}
