// Pure state for the text settings every text form shares: the words, font,
// style, height, alignment and spacing.

import type { TextSpec } from '../../bridge/document';
import type { Units } from '../../bridge/types';
import {
  convertLengthFields,
  fieldError,
  lengthText,
  lengthValue,
  numberValue,
} from '../../lib/formFields';
import { strings } from '../../lib/formStrings';

export type TextAlignment = TextSpec['alignment'];

export interface TextSpecState {
  text: string;
  fontFamily: string;
  bold: boolean;
  italic: boolean;
  /** Cap height in the display unit. */
  size: string;
  alignment: TextAlignment;
  /** Multiple of the font's own line pitch. */
  lineSpacing: string;
  /** Extra space between letters as a percentage of the height. */
  charSpacing: string;
}

export type TextSpecField = 'text' | 'size' | 'lineSpacing' | 'charSpacing';
export type TextSpecErrors = Partial<Record<TextSpecField, string>>;

export function initialTextSpecState(units: Units): TextSpecState {
  return {
    text: 'Text',
    fontFamily: '',
    bold: false,
    italic: false,
    size: lengthText(units === 'inch' ? 25.4 : 20, units),
    alignment: 'left',
    lineSpacing: '1',
    charSpacing: '0',
  };
}

export function convertTextSpecUnits(state: TextSpecState, from: Units, to: Units): TextSpecState {
  return convertLengthFields(state, ['size'], from, to);
}

export function validateTextSpec(state: TextSpecState, units: Units): TextSpecErrors {
  const errors: TextSpecErrors = {};
  if (state.text.trim() === '') {
    errors.text = strings.text.errorEmpty;
  }
  const size = fieldError(lengthValue(state.size, units), { positive: true });
  const line = fieldError(numberValue(state.lineSpacing), { positive: true, max: 10 });
  const chars = fieldError(numberValue(state.charSpacing), { min: -50, max: 500 });
  if (size !== undefined) {
    errors.size = size;
  }
  if (line !== undefined) {
    errors.lineSpacing = line;
  }
  if (chars !== undefined) {
    errors.charSpacing = chars;
  }
  return errors;
}

/** The kernel's text spec, or null while a field is invalid or no font is chosen. */
export function textSpec(state: TextSpecState, units: Units): TextSpec | null {
  if (Object.keys(validateTextSpec(state, units)).length > 0 || state.fontFamily === '') {
    return null;
  }
  return {
    text: state.text,
    fontFamily: state.fontFamily,
    bold: state.bold,
    italic: state.italic,
    sizeMm: lengthValue(state.size, units) ?? 0,
    alignment: state.alignment,
    lineSpacing: numberValue(state.lineSpacing) ?? 1,
    charSpacing: (numberValue(state.charSpacing) ?? 0) / 100,
    kerning: true,
  };
}

/** Form state for an existing text entity's spec, for editing it. */
export function stateFromSpec(spec: TextSpec, units: Units): TextSpecState {
  return {
    text: spec.text,
    fontFamily: spec.fontFamily,
    bold: spec.bold,
    italic: spec.italic,
    size: lengthText(spec.sizeMm, units),
    alignment: spec.alignment,
    lineSpacing: String(spec.lineSpacing),
    charSpacing: String(Math.round(spec.charSpacing * 1000) / 10),
  };
}

/** The family to start with: the one asked for when installed, else a common sans. */
export function defaultFamily(families: readonly string[], wanted = ''): string {
  const find = (name: string) => families.find((f) => f.toLowerCase() === name.toLowerCase());
  return (
    find(wanted) ?? find('Helvetica') ?? find('Arial') ?? find('DejaVu Sans') ?? families[0] ?? ''
  );
}

/** Fonts the font list keeps at its top as recently used. */
export const RECENT_FONTS = 5;

/** The recently used fonts that are still installed, newest first. */
export function shownRecentFonts(recent: readonly string[], families: readonly string[]): string[] {
  return recent.filter((family) => families.includes(family)).slice(0, RECENT_FONTS);
}

/** The recent fonts list once `family` has been picked. */
export function withRecentFont(recent: readonly string[], family: string): string[] {
  if (family === '') {
    return [...recent];
  }
  return [family, ...recent.filter((f) => f !== family)].slice(0, RECENT_FONTS);
}

/** Which fonts the font list shows: all of them, or the single line ones. */
export type FontFilter = 'all' | 'singleLine';

/** The families the font list shows under `filter`. */
export function filteredFamilies(
  families: readonly string[],
  singleLine: readonly string[],
  filter: FontFilter,
): string[] {
  return filter === 'all' ? [...families] : families.filter((f) => singleLine.includes(f));
}

/** The filter to start with: Single Line when the text already uses such a font. */
export function initialFontFilter(family: string, singleLine: readonly string[]): FontFilter {
  return singleLine.includes(family) ? 'singleLine' : 'all';
}
