import { describe, expect, it } from 'vitest';

import { NODE_CURSOR, ROTATE_CURSOR, handleCursor } from './cursors';

describe('interaction cursors', () => {
  it('shows resize arrows on the scale handles', () => {
    expect(handleCursor('nw')).toBe('nwse-resize');
    expect(handleCursor('ne')).toBe('nesw-resize');
    expect(handleCursor('n')).toBe('ns-resize');
    expect(handleCursor('w')).toBe('ew-resize');
  });

  it('shows the rotate cursor in transform mode', () => {
    expect(handleCursor('rotate')).toBe(ROTATE_CURSOR);
    expect(handleCursor('pivot')).toBe('move');
  });

  it('builds image cursors with a fallback', () => {
    expect(NODE_CURSOR.startsWith('url("data:image/svg+xml,')).toBe(true);
    expect(NODE_CURSOR.endsWith(', crosshair')).toBe(true);
    expect(ROTATE_CURSOR.endsWith(', alias')).toBe(true);
  });
});
