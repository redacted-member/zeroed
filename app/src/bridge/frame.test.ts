import { describe, expect, it } from 'vitest';

import { jsonResult, refusalText } from './frame';
import { KernelError } from './types';

describe('kernel refusals', () => {
  it('start with a capital, as a sentence does', () => {
    expect(refusalText('select the closed vectors to use')).toBe(
      'Select the closed vectors to use',
    );
    expect(refusalText('could not read or write /a.carve: denied')).toBe(
      'Could not read or write /a.carve: denied',
    );
  });

  it('leave a message that starts with a name as it is', () => {
    expect(refusalText('a.svg is not a vector file this build can read')).toBe(
      'a.svg is not a vector file this build can read',
    );
    expect(refusalText('')).toBe('');
  });

  it('reach the caller as a KernelError in plain words', () => {
    const body = new TextEncoder().encode(
      JSON.stringify({ ok: false, error: { code: 'validation', message: 'pick a tool first' } }),
    );
    let caught: unknown = null;
    try {
      jsonResult('toolpaths.calculate', { tag: 'JSON', body });
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(KernelError);
    expect((caught as KernelError).message).toBe('Pick a tool first');
    expect((caught as KernelError).code).toBe('validation');
  });
});
