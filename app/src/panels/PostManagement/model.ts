// Pure rules of the Post-Processor Management dialog: which posts are the
// user's own, the order they list in and the filter.

import type { PostChange, PostSummary } from '../../bridge/cam';

/** Whether the user installed or customised this post (it has a file of its own). */
export function isCustom(post: PostSummary): boolean {
  return (post.sourcePath ?? '') !== '';
}

/** Posts matching `filter` in name, id or extension, the user's own first, then by name. */
export function visiblePosts(posts: readonly PostSummary[], filter: string): PostSummary[] {
  const words = filter
    .toLowerCase()
    .split(/\s+/)
    .filter((w) => w !== '');
  const matches = (post: PostSummary) => {
    const haystack = `${post.name} ${post.id} ${post.fileExtension}`.toLowerCase();
    return words.every((word) => haystack.includes(word));
  };
  return posts.filter(matches).sort((a, b) => {
    const custom = Number(isCustom(b)) - Number(isCustom(a));
    if (custom !== 0) {
      return custom;
    }
    return a.name.localeCompare(b.name);
  });
}

/** The features a post supports, as keys of the dialog's strings. */
export function features(post: PostSummary): Array<'arcs' | 'laser' | 'rotary'> {
  const out: Array<'arcs' | 'laser' | 'rotary'> = [];
  if (post.arcs) {
    out.push('arcs');
  }
  if (post.jet === true) {
    out.push('laser');
  }
  if (post.rotary === true) {
    out.push('rotary');
  }
  return out;
}

/** When the post last changed: the date of the newest History row that has one. */
export function lastChanged(entries: readonly PostChange[]): string | null {
  for (let i = entries.length - 1; i >= 0; i -= 1) {
    const when = entries[i]?.when.trim() ?? '';
    if (when !== '') {
      return when;
    }
  }
  return null;
}
