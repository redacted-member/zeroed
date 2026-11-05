// The drawing forms of the Design tab (every group but Modeling Tools), plus
// Text Selection. While one of them draws in the 2D view, Esc or a right
// click ends its tool and closes the form.

import type { FormId } from '../../state/uiStore';
import { TOOL_GROUPS } from './toolGroups';

const DRAWING_FORMS: ReadonlySet<FormId> = new Set<FormId>([
  ...TOOL_GROUPS.design
    .filter((group) => group.id !== 'modelingTools')
    .flatMap((group) => group.tools.flatMap((tool) => tool.form ?? [])),
  'textSelection',
]);

/** Whether the form is one of the Design tab's drawing tools. */
export function isDrawingForm(form: FormId | null): boolean {
  return form !== null && DRAWING_FORMS.has(form);
}
