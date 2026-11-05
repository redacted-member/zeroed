// Pure state for the PDF page choice shown when an imported PDF has more
// than one page: the page field and its check.

/** The page number the field holds, or null while it is not a page of the file. */
export function pageValue(text: string, pages: number): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) {
    return null;
  }
  const page = Number(trimmed);
  return page >= 1 && page <= pages ? page : null;
}

/** The field text after a step up or down, kept inside 1 to `pages`. */
export function stepPage(text: string, pages: number, step: 1 | -1): string {
  const current = pageValue(text, pages) ?? 1;
  return String(Math.min(pages, Math.max(1, current + step)));
}

/** True for files that may hold several pages: PDF and PDF based Illustrator files. */
export function mayHavePages(path: string): boolean {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return ext === 'pdf' || ext === 'ai';
}
