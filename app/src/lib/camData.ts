// The tool database and post processor lists, fetched on first use and
// shared by every toolpath form. Editing the database calls `reloadTools`,
// which every picker follows.

import { useEffect } from 'react';
import { create } from 'zustand';

import { api } from '../bridge/api';
import type { PostSummary, Tool, ToolDatabaseList } from '../bridge/cam';
import { strings } from './strings';

interface CamDataState {
  tools: ToolDatabaseList | null;
  posts: PostSummary[] | null;
  loadingTools: boolean;
  loadingPosts: boolean;
}

const useCamData = create<CamDataState>()(() => ({
  tools: null,
  posts: null,
  loadingTools: false,
  loadingPosts: false,
}));

/** Fetches the tool database again, for after it has been edited. */
export async function reloadTools(): Promise<void> {
  useCamData.setState({ loadingTools: true });
  try {
    useCamData.setState({ tools: await api.tools.list() });
  } catch {
    useCamData.setState({ tools: null });
  } finally {
    useCamData.setState({ loadingTools: false });
  }
}

/** Fetches the post processor list again, after one is installed. */
export function reloadPosts(): Promise<void> {
  return loadPosts();
}

async function loadPosts(): Promise<void> {
  useCamData.setState({ loadingPosts: true });
  try {
    useCamData.setState({ posts: await api.posts.list() });
  } catch {
    useCamData.setState({ posts: [] });
  } finally {
    useCamData.setState({ loadingPosts: false });
  }
}

/** The tool database, or null until it has arrived. */
export function useToolDatabase(): ToolDatabaseList | null {
  const tools = useCamData((state) => state.tools);
  useEffect(() => {
    const state = useCamData.getState();
    if (state.tools === null && !state.loadingTools) {
      void reloadTools();
    }
  }, []);
  return tools;
}

/** The post processors, or null until they have arrived. */
export function usePosts(): PostSummary[] | null {
  const posts = useCamData((state) => state.posts);
  useEffect(() => {
    const state = useCamData.getState();
    if (state.posts === null && !state.loadingPosts) {
      void loadPosts();
    }
  }, []);
  return posts;
}

/** A heading of a tool list and the tools under it. */
export interface ToolListGroup {
  group: { id: number; label: string };
  tools: Tool[];
}

/**
 * The tools grouped for a picker, in database order: tools at the top level
 * first, then each group that holds tools, labelled with its path.
 */
export function toolGroups(database: ToolDatabaseList | null): ToolListGroup[] {
  if (database === null) {
    return [];
  }
  const top = database.tools.filter((t) => (t.group ?? 0) === 0);
  const entries: ToolListGroup[] =
    top.length > 0 ? [{ group: { id: 0, label: strings.toolDatabase.topLevel }, tools: top }] : [];
  for (const group of database.groups) {
    const tools = database.tools.filter((t) => t.group === group.id);
    if (tools.length > 0) {
      entries.push({ group: { id: group.id, label: group.path.join(' / ') }, tools });
    }
  }
  return entries;
}
