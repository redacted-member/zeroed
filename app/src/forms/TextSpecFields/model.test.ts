import { describe, expect, it } from 'vitest';

import {
  defaultFamily,
  filteredFamilies,
  initialFontFilter,
  initialTextSpecState,
  shownRecentFonts,
  stateFromSpec,
  textSpec,
  validateTextSpec,
  withRecentFont,
} from './model';

describe('text spec fields model', () => {
  it('builds a kernel spec once a font is chosen', () => {
    const state = initialTextSpecState('mm');
    expect(textSpec(state, 'mm')).toBeNull();
    expect(textSpec({ ...state, fontFamily: 'Arial', charSpacing: '10' }, 'mm')).toEqual({
      text: 'Text',
      fontFamily: 'Arial',
      bold: false,
      italic: false,
      sizeMm: 20,
      alignment: 'left',
      lineSpacing: 1,
      charSpacing: 0.1,
      kerning: true,
    });
  });

  it('refuses empty text and a zero height', () => {
    const errors = validateTextSpec({ ...initialTextSpecState('mm'), text: ' ', size: '0' }, 'mm');
    expect(errors.text).toBeDefined();
    expect(errors.size).toBeDefined();
  });

  it('round trips a spec through the form state', () => {
    const spec = textSpec({ ...initialTextSpecState('inch'), fontFamily: 'Arial' }, 'inch');
    expect(spec).not.toBeNull();
    if (spec !== null) {
      expect(textSpec(stateFromSpec(spec, 'inch'), 'inch')).toEqual(spec);
    }
  });

  it('picks the wanted family, then a common sans, then the first', () => {
    expect(defaultFamily(['Arial', 'Times'], 'times')).toBe('Times');
    expect(defaultFamily(['Zapf', 'Arial'])).toBe('Arial');
    expect(defaultFamily(['Zapf'])).toBe('Zapf');
    expect(defaultFamily([])).toBe('');
  });
});

describe('recent fonts', () => {
  it('puts the picked font first and keeps five', () => {
    const recent = ['A', 'B', 'C', 'D', 'E'];
    expect(withRecentFont(recent, 'C')).toEqual(['C', 'A', 'B', 'D', 'E']);
    expect(withRecentFont(recent, 'F')).toEqual(['F', 'A', 'B', 'C', 'D']);
    expect(withRecentFont(recent, '')).toEqual(recent);
  });

  it('shows only fonts that are still installed', () => {
    expect(shownRecentFonts(['Gone', 'Arial', 'Times'], ['Arial', 'Georgia', 'Times'])).toEqual([
      'Arial',
      'Times',
    ]);
  });
  it('filters the font list to the single line fonts', () => {
    const families = ['Arial', 'Single Line Sans', 'Single Line Script'];
    const single = ['Single Line Sans', 'Single Line Script'];
    expect(filteredFamilies(families, single, 'all')).toEqual(families);
    expect(filteredFamilies(families, single, 'singleLine')).toEqual(single);
    expect(initialFontFilter('Single Line Sans', single)).toBe('singleLine');
    expect(initialFontFilter('Arial', single)).toBe('all');
  });
});
