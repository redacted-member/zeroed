import { strings } from '../../lib/formStrings';
import { tipText } from '../../shell/Tooltip/model';

type TipKey = keyof typeof strings.form.buttonTips;

/** The icon (a shell/ToolIcons id) and tooltip line for each kind of action button. */
const LOOKS: Readonly<Record<TipKey, string>> = {
  calculate: 'formCalculate',
  apply: 'formApply',
  ok: 'formApply',
  create: 'formCreate',
  close: 'dismiss',
  cancel: 'dismiss',
  preview: 'formPreview',
  reset: 'formReset',
  delete: 'editDelete',
  clear: 'editDelete',
  bake: 'bakeComponents',
  run: 'formRun',
  finish: 'formApply',
  hide: 'hidden',
  copy: 'editCopy',
};

export interface ButtonLook {
  /** A shell/ToolIcons id. */
  icon: string;
  /** The tooltip: the label, then what the button does (or why it is off). */
  tip: string;
}

function tipKey(label: string): TipKey | null {
  const first = label.split(/[\s.]/)[0]?.toLowerCase() ?? '';
  // "Running..." is the Run button while it works.
  const key = first === 'running' ? 'run' : first;
  return Object.hasOwn(LOOKS, key) ? (key as TipKey) : null;
}

/**
 * The icon and tooltip of a form action button, found from its label's
 * first word. A button the list does not know acts with the settings above,
 * so it takes the Apply icon and a tooltip of its name. `why`, given while
 * the button is off, says what it waits for.
 */
export function buttonLook(label: string, why?: string): ButtonLook {
  const key = tipKey(label);
  const line = why ?? (key === null ? undefined : strings.form.buttonTips[key]);
  return { icon: key === null ? 'formApply' : LOOKS[key], tip: tipText(label, undefined, line) };
}
