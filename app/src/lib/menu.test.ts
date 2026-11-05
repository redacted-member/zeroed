import { describe, expect, it } from 'vitest';

import { MENU_COMMAND_EVENT, isMenuCommand } from './menu';

describe('native menu events', () => {
  it('uses an event name Tauri accepts', () => {
    expect(MENU_COMMAND_EVENT).toMatch(/^[A-Za-z0-9\-/:_]+$/);
  });

  it('recognises menu ids from the payload', () => {
    expect(isMenuCommand('edit.jobSetup')).toBe(true);
    expect(isMenuCommand('view.mode3d')).toBe(true);
    expect(isMenuCommand('file.recent.0')).toBe(false);
    expect(isMenuCommand('nonsense')).toBe(false);
  });
});
