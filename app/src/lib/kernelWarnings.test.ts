import { describe, expect, it } from 'vitest';

import type { KernelWarning } from '../bridge/types';
import { warningText, warningTexts } from './kernelWarnings';

const warning = (
  code: string,
  params: Record<string, string | number> = {},
  message = 'English from the kernel.',
): KernelWarning => ({ code, params, message });

describe('warningText', () => {
  it('words a warning from its code and values', () => {
    expect(warningText(warning('toolpath.nothingLeft', { tool: 'End Mill 3 mm' }))).toBe(
      'End Mill 3 mm has nothing left to machine after the tools before it.',
    );
    expect(warningText(warning('chamfer.multiPass', { reach: 2.5 }))).toContain('(2.500 mm)');
    expect(warningText(warning('thread.pitchDiffers', { toolPitch: 1.5, pitch: 2 }))).toBe(
      "The tool's pitch (1.500 mm) differs from the thread's (2.000 mm); only a single point cutter can cut it.",
    );
    expect(warningText(warning('toolpath.keepOut', { zone: 'Clamps' }))).toContain(
      'keep-out zone on layer Clamps',
    );
    expect(warningText(warning('toolpath.cutsThrough', { depth: 26, thickness: 18 }))).toBe(
      'The toolpath cuts 26.000 mm deep, right through the 18.000 mm material. Set a flat depth or a thicker material if that is not meant.',
    );
  });

  it('picks the singular or plural sentence by count', () => {
    expect(warningText(warning('fix.repaired', { count: 1 }))).toBe(
      '1 vector crossed itself and was repaired before machining.',
    );
    expect(warningText(warning('fix.repaired', { count: 4 }))).toBe(
      '4 vectors crossed themselves and were repaired before machining.',
    );
  });

  it('words import warnings', () => {
    expect(warningText(warning('import.imageSkipped'))).toBe('a placed image was skipped');
    expect(warningText(warning('toolpathImport.skipped', { word: 'G41' }))).toBe('G41');
    expect(warningText(warning('toolpathImport.skippedRotary'))).toBe('rotary axes (A, B, C)');
  });

  it('falls back to the kernel English for an unknown code or a missing value', () => {
    expect(warningText(warning('future.code', {}, 'Something new.'))).toBe('Something new.');
    expect(warningText(warning('toolpath.nothingLeft', {}, 'Fallback.'))).toBe('Fallback.');
    expect(warningText(warning('fix.repaired', { count: 'many' }, 'Fallback.'))).toBe('Fallback.');
  });

  it('keeps the order of a reply', () => {
    expect(warningTexts([warning('texture.noStrokes'), warning('pocket.stepoverWide')])).toEqual([
      'No strokes fit inside the vectors.',
      'The stepover is wider than the tool, so ridges will be left standing.',
    ]);
  });
});
