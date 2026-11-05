// The N key toggles node editing. From a drawing tool it goes straight to
// node editing of what was just drawn (the new vector is selected), closing
// the drawing form, instead of leaving the form open under the node tool.

import type { ToolId } from '../state/viewStore';

export interface NodeToggle {
  tool: ToolId;
  closeForm: boolean;
}

export function nodeToggle(activeTool: ToolId, formOpen: boolean): NodeToggle {
  if (activeTool === 'node') {
    return { tool: 'select', closeForm: false };
  }
  return { tool: 'node', closeForm: activeTool === 'create' && formOpen };
}
