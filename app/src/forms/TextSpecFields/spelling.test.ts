import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { TextSpecFields } from './TextSpecFields';
import { initialTextSpecState } from './model';

describe('spell checking in the text fields', () => {
  const render = (multiline: boolean) =>
    renderToStaticMarkup(
      createElement(TextSpecFields, {
        state: initialTextSpecState('mm'),
        setState: () => undefined,
        units: 'mm',
        errors: {},
        multiline,
      }),
    );

  it('spell checks the text box without autocorrecting it', () => {
    for (const markup of [render(true), render(false)]) {
      // HTML attribute names are case insensitive.
      const entry = /<(textarea|input)[^>]*class="text-spec__content"[^>]*>/
        .exec(markup)?.[0]
        ?.toLowerCase();
      expect(entry).toBeDefined();
      expect(entry).toContain('spellcheck="true"');
      expect(entry).toContain('autocorrect="off"');
      expect(entry).toContain('autocapitalize="off"');
    }
  });
});
