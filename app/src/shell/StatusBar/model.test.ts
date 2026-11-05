import { describe, expect, it } from 'vitest';

import { strings } from '../../lib/strings';
import { statusLine } from './model';

describe('status bar line', () => {
  it('says Ready, when nothing else is to be said', () => {
    expect(statusLine('ready', null)).toBe('Ready');
  });

  it('shows the picked tool hint in its place', () => {
    expect(statusLine('ready', 'Click to place the center')).toBe('Click to place the center');
  });

  it('says a kernel not yet up or not responding in plain words over any hint', () => {
    expect(statusLine('connecting', 'hint')).toBe(strings.statusBar.kernelConnecting);
    expect(statusLine('offline', 'hint')).toBe(strings.statusBar.kernelOffline);
    for (const status of ['ready', 'connecting', 'offline'] as const) {
      expect(statusLine(status, null)).not.toMatch(/kernel/i);
    }
  });
});
