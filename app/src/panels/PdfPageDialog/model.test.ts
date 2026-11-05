import { describe, expect, it } from 'vitest';

import { mayHavePages, pageValue, stepPage } from './model';

describe('PDF page choice', () => {
  it('takes whole page numbers inside the file only', () => {
    expect(pageValue('2', 3)).toBe(2);
    expect(pageValue(' 3 ', 3)).toBe(3);
    expect(pageValue('0', 3)).toBeNull();
    expect(pageValue('4', 3)).toBeNull();
    expect(pageValue('1.5', 3)).toBeNull();
    expect(pageValue('', 3)).toBeNull();
  });

  it('steps within the pages', () => {
    expect(stepPage('1', 3, 1)).toBe('2');
    expect(stepPage('3', 3, 1)).toBe('3');
    expect(stepPage('1', 3, -1)).toBe('1');
    expect(stepPage('junk', 3, 1)).toBe('2');
  });

  it('asks only for PDF and Illustrator files', () => {
    expect(mayHavePages('/a/plan.PDF')).toBe(true);
    expect(mayHavePages('logo.ai')).toBe(true);
    expect(mayHavePages('logo.svg')).toBe(false);
  });
});
