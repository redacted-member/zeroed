import ts from 'typescript';
import { describe, expect, it } from 'vitest';

import { whyStrings } from '../../lib/strings/why';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';
import { moveWhy } from './why';

const sources = import.meta.glob<string>('../../**/*.tsx', {
  eager: true,
  query: '?raw',
  import: 'default',
});

function attribute(element: ts.JsxOpeningElement, name: string): ts.JsxAttribute | undefined {
  return element.attributes.properties.find(
    (prop): prop is ts.JsxAttribute => ts.isJsxAttribute(prop) && prop.name.getText() === name,
  );
}

function expressionOf(attr: ts.JsxAttribute | undefined): ts.Expression | undefined {
  const init = attr?.initializer;
  return init !== undefined && ts.isJsxExpression(init) ? init.expression : undefined;
}

function holdsButtonFace(element: ts.JsxElement): boolean {
  let found = false;
  const visit = (node: ts.Node): void => {
    if (
      (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) &&
      node.tagName.getText() === 'ButtonFace'
    ) {
      found = true;
    }
    ts.forEachChild(node, visit);
  };
  element.children.forEach(visit);
  return found;
}

/**
 * Buttons that can be off but explain it another way: the primary form
 * button keeps its tooltip on the slot around it, a tool list row's whole
 * tooltip is the reason, a dropdown is a field, and menu items grey.
 */
const EXPLAINED_ELSEWHERE = [
  '/FormButtons/FormButtons.tsx',
  '/ToolListField/ToolListField.tsx',
  '/ResizableSelect/ResizableSelect.tsx',
  '/MenuBar/MenuBar.tsx',
  '/PopupMenu/PopupMenu.tsx',
];

/**
 * Every tool button that can be off, with what is wrong: its tooltip must
 * be `whyTip(tip, reason)` and it must be off exactly while that reason is
 * given (`disabled={reason !== undefined}`). `faceOnly` limits the check
 * to buttons showing a ButtonFace.
 */
function unexplained(faceOnly: boolean): string[] {
  const found: string[] = [];
  for (const [path, source] of Object.entries(sources)) {
    if (!faceOnly && EXPLAINED_ELSEWHERE.some((file) => path.endsWith(file))) {
      continue;
    }
    const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const visit = (node: ts.Node): void => {
      if (
        ts.isJsxElement(node) &&
        node.openingElement.tagName.getText() === 'button' &&
        attribute(node.openingElement, 'disabled') !== undefined &&
        (!faceOnly || holdsButtonFace(node))
      ) {
        const where = `${path}:${file.getLineAndCharacterOfPosition(node.getStart()).line + 1}`;
        const tip = expressionOf(attribute(node.openingElement, 'data-tip'));
        const off = expressionOf(attribute(node.openingElement, 'disabled'));
        if (
          tip === undefined ||
          !ts.isCallExpression(tip) ||
          tip.expression.getText() !== 'whyTip' ||
          tip.arguments.length !== 2
        ) {
          found.push(`${where} tooltip is not whyTip(tip, reason)`);
        } else if (off?.getText() !== `${tip.arguments[1]?.getText() ?? ''} !== undefined`) {
          found.push(`${where} is not off exactly while its reason is given`);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
  }
  return found;
}

describe('why a button is off', () => {
  it('gives every button face that can be off a reason in its tooltip', () => {
    expect(unexplained(true)).toEqual([]);
  });

  it('gives every other tool button that can be off a reason in its tooltip', () => {
    expect(unexplained(false)).toEqual([]);
  });

  it('puts the reason in place of the description while the button is off', () => {
    expect(whyTip('Copy\nMakes a copy.', undefined)).toBe('Copy\nMakes a copy.');
    expect(whyTip('Copy\nMakes a copy.', 'Pick a tool first.')).toBe('Copy\nPick a tool first.');
    expect(whyTip('Copy', 'Pick a tool first.')).toBe('Copy\nPick a tool first.');
  });

  it('gives the reason of the first condition that holds', () => {
    expect(firstWhy([false, 'a'], [true, 'b'], [true, 'c'])).toBe('b');
    expect(firstWhy([false, 'a'])).toBeUndefined();
    expect(firstWhy()).toBeUndefined();
  });

  it('says why a row cannot move further', () => {
    const why = whyStrings.why;
    expect(moveWhy(0, 3, -1)).toBe(why.atTop);
    expect(moveWhy(0, 3, 1)).toBeUndefined();
    expect(moveWhy(2, 3, 1)).toBe(why.atBottom);
    expect(moveWhy(-1, 3, 1)).toBe(why.pickInList);
  });

  it('writes every reason as a sentence', () => {
    for (const reason of Object.values(whyStrings.why)) {
      const text =
        typeof reason === 'function'
          ? (reason as unknown as (value: string) => string)('X')
          : reason;
      expect(text).toMatch(/^[A-Z].*\.$/);
    }
  });
});
