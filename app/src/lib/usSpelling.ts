// British spellings the interface must not use: it is written in US English.
// Shared by the spelling tests of the strings and of the help content.

export const BRITISH =
  /centre|colour|\bgrey|modell|mould|metre|licence|behaviour|favourite|optimis|organis|analys(?:e|ed|ing)\b|neighbour|cancell(?:ed|ing)|labell(?:ed|ing)|fibre|aluminium|dialogue|catalogue|anticlockwise|whilst|programme|travell(?:ed|ing)|stabilis/i;

/** The words of `text` a user must not read: British spellings, em dashes, other product names. */
export function contentProblems(text: string): string[] {
  const problems: string[] = [];
  if (BRITISH.test(text)) {
    problems.push(`British spelling: ${text}`);
  }
  if (text.includes('—')) {
    problems.push(`em dash: ${text}`);
  }
  return problems;
}
