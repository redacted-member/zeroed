import ts from 'typescript';
import { describe, expect, it } from 'vitest';

import { BRITISH } from '../usSpelling';

// The interface is written in US English. Property names are identifiers
// (some keep older spellings because files and prefs use them), so only the
// text of string and template literals is checked.

const sources = import.meta.glob<string>('./*.ts', {
  eager: true,
  query: '?raw',
  import: 'default',
});

/** The text of every string literal in `source` that is not a property name. */
function literalTexts(source: string): string[] {
  const file = ts.createSourceFile('strings.ts', source, ts.ScriptTarget.Latest, true);
  const texts: string[] = [];
  const visit = (node: ts.Node): void => {
    const isName =
      (ts.isPropertyAssignment(node.parent) && node.parent.name === node) ||
      ts.isImportDeclaration(node.parent);
    if (
      !isName &&
      (ts.isStringLiteral(node) ||
        ts.isNoSubstitutionTemplateLiteral(node) ||
        ts.isTemplateHead(node) ||
        ts.isTemplateMiddle(node) ||
        ts.isTemplateTail(node))
    ) {
      texts.push(node.text);
    }
    ts.forEachChild(node, visit);
  };
  ts.forEachChild(file, visit);
  return texts;
}

describe('US spelling in UI strings', () => {
  it('finds the string files', () => {
    expect(Object.keys(sources).length).toBeGreaterThan(10);
  });

  it('reads literals but not property names', () => {
    expect(literalTexts("const a = { colour: 'Color', b: `x ${1} centre` };")).toEqual([
      'Color',
      'x ',
      ' centre',
    ]);
  });

  it.each(Object.entries(sources).filter(([path]) => !path.endsWith('.test.ts')))(
    '%s has no British spellings',
    (_path, source) => {
      expect(literalTexts(source).filter((text) => BRITISH.test(text))).toEqual([]);
    },
  );
});
