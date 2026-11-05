import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';

import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { IconButton } from '../../forms/IconButton/IconButton';
import { toolDescriptions } from '../../lib/strings/toolDescriptions';
import { TOOL_GROUPS } from '../LeftPanel/toolGroups';
import { loadExtraIcons, toolIcon } from '../ToolIcons/ToolIcons';
import { toolTip } from '../ToolGroups/model';
import { ToolGroups } from '../ToolGroups/ToolGroups';
import { boxAt, DISABLED_TIP, placeTip, tipAllowed, tipText } from './model';

const sources = import.meta.glob<string>('../../**/*.tsx', {
  eager: true,
  query: '?raw',
  import: 'default',
});

type Opening = ts.JsxOpeningElement | ts.JsxSelfClosingElement;

function attribute(element: Opening, name: string): ts.JsxAttribute | undefined {
  return element.attributes.properties.find(
    (prop): prop is ts.JsxAttribute => ts.isJsxAttribute(prop) && prop.name.getText() === name,
  );
}

/** Whether a button's content is only an icon or a glyph, with no words to read. */
function iconOnly(element: ts.JsxElement): boolean {
  let words = false;
  const visit = (node: ts.Node): void => {
    if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
      if (node.tagName.getText() === 'svg') {
        return;
      }
    } else if (
      ts.isCallExpression(node) &&
      /^(toolIcon|drawIcon)$/.test(node.expression.getText())
    ) {
      return;
    } else if (
      ts.isBinaryExpression(node) &&
      node.operatorToken.kind === ts.SyntaxKind.QuestionQuestionToken
    ) {
      // An icon with a fallback text shows the icon.
      visit(node.left);
      return;
    } else if (ts.isConditionalExpression(node)) {
      // The condition picks the content; only the two outcomes show.
      visit(node.whenTrue);
      visit(node.whenFalse);
      return;
    } else if (ts.isJsxText(node)) {
      words ||= /\p{L}{2}/u.test(node.getText().replace(/&\w+;/g, ''));
    } else if (ts.isStringLiteral(node)) {
      words ||= /\p{L}{2}/u.test(node.text);
    } else if (ts.isPropertyAccessExpression(node) || ts.isIdentifier(node)) {
      // Text from strings or a name: a word button.
      words = true;
      return;
    }
    ts.forEachChild(node, visit);
  };
  for (const child of element.children) {
    visit(child);
  }
  return !words;
}

interface Finding {
  file: string;
  line: number;
}

/** Scans every component for icon controls without tooltip text and native title tips. */
function scan(): { untipped: Finding[]; titled: Finding[] } {
  const untipped: Finding[] = [];
  const titled: Finding[] = [];
  for (const [path, source] of Object.entries(sources)) {
    const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const at = (node: ts.Node) => ({
      file: path,
      line: file.getLineAndCharacterOfPosition(node.getStart()).line + 1,
    });
    const visit = (node: ts.Node): void => {
      if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
        const tag = node.tagName.getText();
        if (/^[a-z]/.test(tag) && attribute(node, 'title') !== undefined) {
          titled.push(at(node));
        }
        const role = attribute(node, 'role')?.initializer?.getText();
        const button = tag === 'button' || role === '"button"';
        if (
          button &&
          ts.isJsxOpeningElement(node) &&
          iconOnly(node.parent) &&
          attribute(node, 'data-tip') === undefined
        ) {
          untipped.push(at(node));
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
  }
  return { untipped, titled };
}

/**
 * Buttons with words that show no icon, each with the reason it keeps words
 * only. Keyed by file and the button's first content, as the failing test
 * prints them. Every other button with a label shows an icon beside it.
 */
const WORDS_ONLY: Readonly<Record<string, string>> = {
  'panels/HelpDialog/HelpDialog.tsx: {t.title}': 'a contents row that opens a help page',
  'panels/HelpDialog/HelpDialog.tsx: {findTopic(id).title}': 'a See also link in a help page',
  'forms/AssemblyExplodeSteps/AssemblyExplodeSteps.tsx: {`${index + 1}. ${step.name}`}':
    'a list row that picks a step',
  'forms/AssemblyFit/AssemblyFit.tsx: <span className="assembly-fit__joint-head"> <span className=':
    "a list row that opens a joint's findings",
  'forms/AssemblyMates/AssemblyMates.tsx: <span className="assembly-mates__label">{line}</span>':
    'a list row that picks a mate',
  'forms/AssemblyPieces/AssemblyPieces.tsx: <span aria-hidden="true" className={ reliefs[key] === undefi':
    'a list row that picks a piece; its relief thumbnail is its picture',
  'forms/DrillBankTemplateDialog/DrillBankTemplateDialog.tsx: <circle cx={cx} cy={cy} r={CELL / 2 - 3} />':
    'a hole in the drill bank picture; the picture is the button',
  "forms/JointCreatorForm/JointCreatorForm.tsx: {text.row(text.kinds[row.kind].name, row.id, row.ok ? '' : t":
    'a list row that picks a joint',
  'forms/OptionTabs/OptionTabs.tsx: {tab.label}': 'a page tab, named by its label',
  'forms/ResizableSelect/ResizableSelect.tsx: <span className="resizable-select__value">{current?.label ??':
    "a drop down list's closed face, showing the chosen value",
  'forms/SaveToolpathsForm/SaveToolpathsForm.tsx: {toolpath.name}':
    'a list row that picks a toolpath',
  'forms/SegmentModelForm/SegmentModelForm.tsx: {name}': 'a list row that picks a part',
  'forms/ToolListField/ToolListField.tsx: <span className="tool-list__name">{nameOf(t)}</span>':
    'a list row that picks a tool',
  'panels/ClipartPanel/ClipartPanel.tsx: <span className="clipart-panel__thumb"> {url === undefined ?':
    'a clipart tile; its thumbnail is its picture',
  'panels/FurnitureLibrary/FurnitureLibrary.tsx: <span>{categoryLabel(c.id)}</span>':
    'a list row that picks a category',
  'panels/FurnitureLibrary/FurnitureLibrary.tsx: <span className="furniture-library__thumb"> {thumbnail === u':
    'a template card; its thumbnail is its picture',
  'panels/ComponentsPanel/ComponentsPanel.tsx: <span className="components-panel__label">{component.name}</':
    'a tree row that picks a component, group or level',
  'panels/ComponentsPanel/ComponentsPanel.tsx: <span className="components-panel__label">{group.name}</span':
    'a tree row that picks a component, group or level',
  'panels/ComponentsPanel/ComponentsPanel.tsx: <span className="components-panel__label">{level.name}</span':
    'a tree row that picks a component, group or level',
  'panels/CustomSheetSelection/CustomSheetSelection.tsx: {name}':
    'a table cell that opens its value for editing',
  "panels/CustomSheetSelection/CustomSheetSelection.tsx: {row.quantity.trim() === '' ? text.noLimit : row.quantity}":
    'a table cell that opens its value for editing',
  'panels/LayersPanel/LayersPanel.tsx: <span className="layers-panel__name-text">{layer.name}</span':
    'a list row that picks a layer',
  'panels/MaterialManagement/MaterialManagement.tsx: {material}':
    'a list row that picks a material',
  'panels/PostManagement/PostManagement.tsx: {row.name}': 'a list row that picks a post-processor',
  'panels/Preferences/Preferences.tsx: {text.sectionNames[id]}':
    'a page in the list of sections, a tab',
  'panels/ToolDatabase/ToolDatabase.tsx: {group.name}':
    'a tree row that picks a tool group or tool',
  'panels/ToolDatabase/ToolDatabase.tsx: {tool.displayName ?? tool.name}':
    'a tree row that picks a tool group or tool',
  'panels/ToolpathsPanel/ToolpathsPanel.tsx: <span className="toolpaths-panel__label">{toolpath.name}</sp':
    'a list row that picks a toolpath or group',
  'panels/ToolpathsPanel/ToolpathsPanel.tsx: {group.name}':
    'a list row that picks a toolpath or group',
  'shell/DockButtons/DockButtons.tsx: <span aria-hidden="true" className="dock-buttons__text"> ? <':
    'a glyph button (?), no words',
  'shell/DockTabs/DockTabs.tsx: <span className="dock-tabs__label">{label}</span>':
    'a panel tab, named by its label',
  'shell/MenuBar/MenuBar.tsx: {menu.label}': 'a menu bar title; words only',
  'shell/RightPanel/RightPanel.tsx: <span className="right-dock__tab-label">{strings.tabs.toolpa':
    'a panel tab, named by its label',
  'views/SideSwitch/SideSwitch.tsx: {sideLabel(side)}': 'a side tab (Top, Bottom)',
  'views/SnapBar/SnapBar.tsx: <span aria-hidden="true">{\'\\u00bb\'}</span>':
    'a glyph button (more), no words',
  'views/ViewCube/ViewCube.tsx: <polygon className={tone(facing)} points={points} />':
    'a face of the view cube; the face is the picture',
};

/** Whether a button's content draws an icon: a ButtonFace, an icon or glyph component, or an icon call. */
function showsIcon(element: ts.JsxElement): boolean {
  let icon = false;
  const visit = (node: ts.Node): void => {
    if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
      icon ||= /^svg$|(Face|Icon|Glyph|Gutter)$/.test(node.tagName.getText());
    } else if (ts.isCallExpression(node)) {
      icon ||= /^(toolIcon|drawIcon|gutter)$/.test(node.expression.getText());
    } else if (ts.isIdentifier(node)) {
      icon ||= /icon/i.test(node.text);
    }
    ts.forEachChild(node, visit);
  };
  for (const child of element.children) {
    visit(child);
  }
  return icon;
}

/** A source path from the glob, relative to app/src. */
function appPath(path: string): string {
  return path.startsWith('../../') ? path.slice(6) : `shell/${path.slice(3)}`;
}

/** Every button with a label but no icon, as `file: first content`. */
function wordsOnly(): string[] {
  const found: string[] = [];
  for (const [path, source] of Object.entries(sources)) {
    if (path.includes('.test.')) {
      continue;
    }
    const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
    const visit = (node: ts.Node): void => {
      if (ts.isJsxOpeningElement(node)) {
        const role = attribute(node, 'role')?.initializer?.getText();
        const button = node.tagName.getText() === 'button' || role === '"button"';
        if (button && !iconOnly(node.parent) && !showsIcon(node.parent)) {
          const first = node.parent.children.find(
            (child) => !(ts.isJsxText(child) && child.getText().trim() === ''),
          );
          const head = (first?.getText() ?? '').replace(/\s+/g, ' ').trim();
          found.push(`${appPath(path)}: ${head.slice(0, 60)}`);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
  }
  return found;
}

/** The icon ids ButtonFace is given as string literals anywhere in the app. */
function faceIcons(): string[] {
  const ids: string[] = [];
  for (const source of Object.values(sources)) {
    for (const match of source.matchAll(/<ButtonFace\s+icon=\{?([^}/]*?)\}?\s*(?:label|\/>)/g)) {
      ids.push(...[...(match[1] ?? '').matchAll(/['"](\w+)['"]/g)].map((id) => id[1] ?? ''));
    }
  }
  return ids;
}

describe('tooltips', () => {
  it('gives every icon or glyph button in the app tooltip text', () => {
    const { untipped } = scan();
    expect(untipped).toEqual([]);
  });

  it('uses the shared tooltip, never the native title attribute', () => {
    const { titled } = scan();
    expect(titled).toEqual([]);
  });

  it('shows an icon beside the label of every button, bar list rows and tabs', () => {
    const found = wordsOnly();
    expect(found.filter((key) => WORDS_ONLY[key] === undefined)).toEqual([]);
    // An entry whose button is gone or has an icon now leaves the list.
    expect(Object.keys(WORDS_ONLY).filter((key) => !found.includes(key))).toEqual([]);
  });

  it('draws every icon a button face names', async () => {
    await loadExtraIcons();
    const ids = faceIcons();
    expect(ids.length).toBeGreaterThan(100);
    expect(ids.filter((id) => toolIcon(id) === null)).toEqual([]);
  });

  it('draws a button face as an icon beside its label', () => {
    const face = renderToStaticMarkup(
      createElement(ButtonFace, { icon: 'dismiss', label: 'Close' }),
    );
    expect(face).toContain('button-face__icon');
    expect(face).toContain('<span class="button-face__label">Close</span>');
    const bare = renderToStaticMarkup(createElement(ButtonFace, { icon: 'dismiss' }));
    expect(bare).not.toContain('button-face__label');
  });

  it('gives every Design tab and Toolpaths panel tool its name and key', () => {
    const tools = Object.values(TOOL_GROUPS)
      .flat()
      .flatMap((group) => group.tools);
    const missing = tools.filter((tool) => !toolTip(tool).startsWith(tool.label));
    expect(missing).toEqual([]);
    const tip = (id: string) => toolTip(tools.find((tool) => tool.id === id) ?? tools[0]);
    expect(tip('move')).toBe('Move Selection (M)');
    expect(tip('nodeEditing')).toMatch(/\(N\)$/);
    expect(tip('drawCircle')).toBe('Draw Circle');
  });

  it('gives every Design tab and Toolpaths panel tool a one line description', () => {
    const tools = Object.values(TOOL_GROUPS)
      .flat()
      .flatMap((group) => group.tools);
    const missing = tools.filter((tool) => toolDescriptions[tool.id] === undefined);
    expect(missing.map((tool) => tool.id)).toEqual([]);
    const ids = new Set(tools.map((tool) => tool.id));
    expect(Object.keys(toolDescriptions).filter((id) => !ids.has(id))).toEqual([]);
    for (const line of Object.values(toolDescriptions)) {
      expect(line).toMatch(/^[A-Z][^\n]{10,90}\.$/);
    }
    const tip = (id: string) =>
      toolTip(tools.find((tool) => tool.id === id) ?? tools[0], toolDescriptions);
    expect(tip('move')).toBe(`Move Selection (M)\n${toolDescriptions.move}`);
    expect(tip('drawCircle').split('\n')).toEqual(['Draw Circle', toolDescriptions.drawCircle]);
  });

  it('renders a tooltip on every tool grid button', () => {
    for (const groups of Object.values(TOOL_GROUPS)) {
      const markup = renderToStaticMarkup(createElement(ToolGroups, { groups }));
      const buttons = markup.match(/<button[^>]*>/g) ?? [];
      expect(buttons.length).toBeGreaterThan(0);
      expect(buttons.filter((button) => !/data-tip="[^"]+"/.test(button))).toEqual([]);
    }
  });

  it('shows the full name of an icon button, or its label', () => {
    const onClick = () => undefined;
    const full = renderToStaticMarkup(
      createElement(IconButton, { icon: 'move', label: 'Move', title: 'Move it', onClick }),
    );
    expect(full).toContain('data-tip="Move it"');
    expect(full).toContain('aria-label="Move it"');
    const short = renderToStaticMarkup(
      createElement(IconButton, { icon: 'move', label: 'Move', onClick }),
    );
    expect(short).toContain('data-tip="Move"');
    expect(short).not.toContain('title=');
    const described = renderToStaticMarkup(
      createElement(IconButton, {
        icon: 'sheetAdd',
        label: 'Add New',
        title: 'Add New Sheet',
        description: 'Adds a sheet.',
        onClick,
      }),
    );
    expect(described).toContain('data-tip="Add New Sheet\nAdds a sheet."');
    expect(described).toContain('aria-label="Add New Sheet"');
  });
});

describe('tipText', () => {
  it('adds the key in brackets and the description on its own line', () => {
    expect(tipText('Move')).toBe('Move');
    expect(tipText('Move', 'M')).toBe('Move (M)');
    expect(tipText('Move', undefined, 'Moves the selection.')).toBe('Move\nMoves the selection.');
  });
});

describe('tipAllowed', () => {
  it('holds tooltips off after a click until the pointer moves, but not keyboard focus', () => {
    expect(tipAllowed(false, false)).toBe(true);
    expect(tipAllowed(true, false)).toBe(false);
    expect(tipAllowed(true, true)).toBe(true);
  });
});

describe('placeTip', () => {
  const view = { width: 800, height: 600 };
  const size = { width: 100, height: 20 };

  it('centers the tooltip below its control', () => {
    expect(placeTip({ left: 100, top: 100, width: 20, height: 20 }, size, view)).toEqual({
      left: 60,
      top: 126,
    });
  });

  it('goes above a control at the bottom and stays inside the window', () => {
    expect(placeTip({ left: 0, top: 580, width: 20, height: 20 }, size, view)).toEqual({
      left: 6,
      top: 554,
    });
    expect(placeTip({ left: 790, top: 10, width: 10, height: 10 }, size, view).left).toBe(694);
  });
});

describe('boxAt', () => {
  it('finds the disabled control under the pointer, the one drawn on top', () => {
    const off = { left: 10, top: 10, width: 80, height: 24 };
    const inner = { left: 20, top: 12, width: 20, height: 20 };
    expect(boxAt([off], 50, 20)).toBe(0);
    expect(boxAt([off, inner], 25, 20)).toBe(1);
    expect(boxAt([off], 90, 20)).toBe(-1);
    expect(boxAt([], 50, 20)).toBe(-1);
    expect(DISABLED_TIP).toBe('button:disabled[data-tip]');
  });
});
