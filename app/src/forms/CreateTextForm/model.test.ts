import { describe, expect, it } from 'vitest';

import { DEFAULT_JOB } from '../../bridge/types';
import {
  adjustmentsOf,
  anchorHandle,
  initialTextPosition,
  positionAt,
  startAnchor,
  textPosition,
  validateTextPosition,
  withAdjustments,
  wrapWidth,
} from './model';

describe('create text model', () => {
  it('starts at the bottom left anchor unless the last used one is asked for', () => {
    expect(initialTextPosition('mm', DEFAULT_JOB).anchor).toBe('bottomLeft');
    expect(startAnchor({ lastTextAnchor: false, textAnchor: 'centre' })).toBe('bottomLeft');
    expect(startAnchor({ lastTextAnchor: true, textAnchor: 'centre' })).toBe('centre');
  });

  it('starts inside the job', () => {
    const at = textPosition(initialTextPosition('mm', DEFAULT_JOB), 'mm');
    expect(at).toEqual({ x: DEFAULT_JOB.width * 0.1, y: DEFAULT_JOB.height / 2 });
  });

  it('takes a clicked point and reads it back in millimetres', () => {
    const start = initialTextPosition('inch', DEFAULT_JOB);
    expect(textPosition(positionAt(start, { x: 25.4, y: 50.8 }, 'inch'), 'inch')).toEqual({
      x: 25.4,
      y: 50.8,
    });
  });

  it('flags a position that is not a number', () => {
    const start = initialTextPosition('mm', DEFAULT_JOB);
    expect(validateTextPosition({ ...start, x: 'a' }, 'mm').x).toBeDefined();
    expect(textPosition({ ...start, x: 'a' }, 'mm')).toBeNull();
  });

  it('wraps only when asked, to a positive width', () => {
    const plain = initialTextPosition('mm', DEFAULT_JOB);
    expect(wrapWidth(plain, 'mm')).toBeUndefined();
    const block = initialTextPosition('mm', DEFAULT_JOB, true);
    expect(wrapWidth(block, 'mm')).toBe(DEFAULT_JOB.width / 2);
    expect(wrapWidth({ ...block, width: '0' }, 'mm')).toBeNull();
    expect(validateTextPosition({ ...block, width: '-3' }, 'mm').width).toBeDefined();
    expect(validateTextPosition({ ...plain, width: '-3' }, 'mm').width).toBeUndefined();
  });

  it('keeps the wrap settings when the position moves', () => {
    const block = initialTextPosition('mm', DEFAULT_JOB, true);
    const moved = positionAt(block, { x: 1, y: 2 }, 'mm');
    expect(moved.wrap).toBe(true);
    expect(moved.width).toBe(block.width);
  });

  it('sizes the anchor handle in pixels round the anchor', () => {
    const handle = anchorHandle({ x: 10, y: 20 }, 2);
    const ring = handle.subpaths[0];
    const xs = (ring?.segments ?? []).map((s) => s.to.x);
    expect(Math.min(...xs)).toBeCloseTo(8);
    expect(Math.max(...xs)).toBeCloseTo(12);
  });

  it('keeps kerning, line gaps and arch through field edits', () => {
    const spec = {
      text: 'AB',
      fontFamily: 'Sans',
      bold: false,
      italic: false,
      sizeMm: 10,
      alignment: 'left' as const,
      lineSpacing: 1,
      charSpacing: 0,
      kerning: true,
    };
    const kept = adjustmentsOf({ ...spec, letterGaps: [[0.1]], arch: 5 });
    expect(kept).toEqual({ letterGaps: [[0.1]], arch: 5 });
    const edited = withAdjustments({ ...spec, text: 'ABC' }, kept);
    expect(edited.text).toBe('ABC');
    expect(edited.arch).toBe(5);
    expect(edited.letterGaps).toEqual([[0.1]]);
    expect(withAdjustments(edited, {})).not.toHaveProperty('arch');
  });
});

describe('typing after placing the text', () => {
  const sources = import.meta.glob<string>('./CreateTextForm.tsx', {
    eager: true,
    query: '?raw',
    import: 'default',
  });
  const source = Object.values(sources)[0] ?? '';

  it('moves the focus into the text box when the form opens and after each placing click', () => {
    // The box takes the focus once on opening (serial 1) ...
    expect(source).toMatch(/useState\(1\)/);
    // ... and again after a click places text or draws its box, so the
    // letters typed next go into the text, not to the view as shortcuts.
    const placed = source.split('setFocusSerial((serial) => serial + 1)').length - 1;
    expect(placed).toBe(2);
    expect(source).toContain('focusSerial={focusSerial}');
  });
});
