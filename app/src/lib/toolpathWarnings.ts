// The warnings a toolpath calculation leaves, gathered for the dialog that
// shows them once the calculation is done.

/** What one calculation said, for the toolpath it made (null: no name). */
export interface CalculationWarnings {
  toolpath: string | null;
  warnings: readonly string[];
}

/** The warnings of one toolpath, as the dialog lists them. */
export interface ToolpathWarning {
  /** The toolpath the messages are about, or null when not one toolpath. */
  toolpath: string | null;
  messages: string[];
}

/**
 * Gathers the warnings of one or more calculations: blank messages and
 * repeats of a message for the same toolpath are dropped, toolpaths with
 * nothing to say are left out, and a toolpath calculated twice keeps one
 * entry. The order is the order of the calculations.
 */
export function gatherWarnings(results: readonly CalculationWarnings[]): ToolpathWarning[] {
  const out: ToolpathWarning[] = [];
  for (const result of results) {
    const messages = result.warnings.map((m) => m.trim()).filter((m) => m !== '');
    if (messages.length === 0) {
      continue;
    }
    let entry = out.find((e) => e.toolpath === result.toolpath);
    if (entry === undefined) {
      entry = { toolpath: result.toolpath, messages: [] };
      out.push(entry);
    }
    for (const message of messages) {
      if (!entry.messages.includes(message)) {
        entry.messages.push(message);
      }
    }
  }
  return out;
}

/** How many messages there are in all. */
export function warningCount(warnings: readonly ToolpathWarning[]): number {
  return warnings.reduce((sum, w) => sum + w.messages.length, 0);
}

/** The name of toolpath `id` in `list`, or null when it is not there. */
export function toolpathName(
  list: readonly { id: number; name: string }[],
  id: number,
): string | null {
  return list.find((t) => t.id === id)?.name ?? null;
}
