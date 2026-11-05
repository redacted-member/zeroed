// Pure rules of the Immediate Output block: which toolpath it saves, what
// stops it, and the name the save dialog suggests.

import type { RotaryWiring } from '../../bridge/prefs';
import type { PostSummary, SaveToolpathsArgs, ToolpathSummary } from '../../bridge/cam';
import { strings } from '../../lib/formStrings';
import { capabilityProblem, suggestedName } from '../SaveToolpathsForm/model';

const text = strings.toolpaths.immediate;

/** The toolpath the form calculated last, while it is still in the list. */
export function immediateToolpath(
  toolpaths: readonly ToolpathSummary[],
  id: number | null,
): ToolpathSummary | null {
  return id === null ? null : (toolpaths.find((t) => t.id === id) ?? null);
}

/** Why Save Toolpath(s) is greyed, or null when it can save. */
export function immediateProblem(
  toolpath: ToolpathSummary | null,
  post: PostSummary | null,
): string | null {
  if (toolpath === null) {
    return text.notCalculated;
  }
  if (post === null) {
    return text.noPost;
  }
  return capabilityProblem([toolpath], post);
}

/** The file name offered for the toolpath, with the side when asked. */
export function immediateFileName(
  toolpath: ToolpathSummary,
  post: PostSummary,
  side: string | null,
): string {
  return suggestedName('selected', [toolpath], post.fileExtension, side);
}

/** The cam.save arguments that write the one toolpath. */
export function immediateSaveArgs(
  toolpath: ToolpathSummary,
  post: PostSummary,
  path: string,
  wiring?: RotaryWiring,
): SaveToolpathsArgs {
  return {
    toolpathIds: [toolpath.id],
    post: post.id,
    path,
    output: 'selected',
    ...(wiring === undefined ? {} : { rotaryWiring: wiring }),
  };
}
