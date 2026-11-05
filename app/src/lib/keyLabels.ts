// Keys as the platform writes them. On macOS every shortcut the user sees
// uses the key symbols (Control, Option, Shift, Command: ⌃ ⌥ ⇧ ⌘), in that
// order and run together (⇧⌘Z); elsewhere the names joined by plus signs
// (Ctrl+Shift+Z). The app's Ctrl shortcuts are Command shortcuts on the
// Mac. Every shortcut shown in text, tooltips, menus and dialogs goes
// through this file.

/** True on macOS (and the other Apple platforms). */
export const IS_MAC =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? '');

/** A key and the modifiers held with it. */
export interface KeyCombo {
  /** `KeyboardEvent.key`: a letter, or a name such as F3 or PageUp. */
  key: string;
  /** The app's main modifier: Ctrl, and Command on the Mac. */
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
  /** The Control key itself on the Mac (Ctrl elsewhere). */
  control?: boolean;
}

type Modifier = 'ctrl' | 'shift' | 'alt' | 'control';

/** The modifiers in the order each platform writes them. */
const MAC_ORDER: readonly [Modifier, string][] = [
  ['control', '⌃'],
  ['alt', '⌥'],
  ['shift', '⇧'],
  ['ctrl', '⌘'],
];
const OTHER_ORDER: readonly [Modifier, string][] = [
  ['ctrl', 'Ctrl'],
  ['control', 'Ctrl'],
  ['shift', 'Shift'],
  ['alt', 'Alt'],
];

/** Names a spec such as `Ctrl+Shift+F` may use for each modifier. */
const SPEC_NAMES: Readonly<Record<string, Modifier>> = {
  ctrl: 'ctrl',
  cmd: 'ctrl',
  shift: 'shift',
  alt: 'alt',
  option: 'alt',
  control: 'control',
};

/**
 * The label of `combo`: `⇧⌘Z` on the Mac, `Ctrl+Shift+Z` elsewhere. A one
 * letter key is written in capitals; a named key through `names` (Delete
 * as Del, say) and, on the Mac, apart from the symbols (`⌘ click`). With
 * no key, only the modifiers.
 */
export function comboLabel(
  combo: KeyCombo,
  mac: boolean = IS_MAC,
  names: Readonly<Record<string, string>> = {},
): string {
  const key = combo.key.length === 1 ? combo.key.toUpperCase() : (names[combo.key] ?? combo.key);
  const held = (mac ? MAC_ORDER : OTHER_ORDER)
    .filter(([modifier]) => combo[modifier] === true)
    .map(([, label]) => label);
  const unique = held.filter((label, index) => held.indexOf(label) === index);
  if (mac) {
    const symbols = unique.join('');
    if (key === '') {
      return symbols;
    }
    return symbols === '' || key.length === 1 ? `${symbols}${key}` : `${symbols} ${key}`;
  }
  return [...unique, key].filter((part) => part !== '').join('+');
}

/**
 * A combination written the Windows way, `Ctrl+Shift+F`, as the platform
 * writes it: `⇧⌘F` on the Mac. Modifier names are Ctrl (Command on the
 * Mac), Shift, Alt and Control; what follows them is the key, which may be
 * a word (`Ctrl+click` gives `⌘ click` on the Mac) or left out (`Ctrl`
 * gives `⌘`).
 */
export function keys(spec: string, mac: boolean = IS_MAC): string {
  const combo: KeyCombo = { key: '' };
  const rest: string[] = [];
  for (const part of spec.split('+')) {
    const modifier = rest.length === 0 ? SPEC_NAMES[part.trim().toLowerCase()] : undefined;
    if (modifier === undefined) {
      rest.push(part);
    } else {
      combo[modifier] = true;
    }
  }
  combo.key = rest.join('+').trim();
  return comboLabel(combo, mac);
}

/** The main modifier as the platform writes it in text: ⌘ or Ctrl. */
export const CTRL = keys('Ctrl');
/** Alt as the platform writes it in text: ⌥ or Alt. */
export const ALT = keys('Alt');
/** Shift as the platform writes it in text: ⇧ or Shift. */
export const SHIFT = keys('Shift');
