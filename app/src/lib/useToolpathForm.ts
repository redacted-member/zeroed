// What every toolpath form shares: the tool it cuts with, the toolpath it is
// editing (loaded back into the form), and calculating from the selection.

import { useCallback, useEffect, useState } from 'react';

import { api } from '../bridge/formApi';
import type { StrategyName, Tool, ToolpathRequest } from '../bridge/cam';
import type { VectorFilter } from '../bridge/sketch';
import { foldMaxPassDepth, foldToolOverrides } from '../forms/ToolEditDialog/model';
import { isAutomatic, withSelector } from '../forms/VectorSelectionField/model';
import { useDocumentStore } from '../state/documentStore';
import { toolpathsStrings } from './strings/toolpaths';
import { useToolpathStore } from '../state/toolpathStore';
import { useUiStore } from '../state/uiStore';
import { nextToolpathName, TOOLPATH_BASE_NAMES } from './toolpathNames';

/**
 * Why a toolpath form's Calculate is off: no vectors to machine comes first
 * (the form says so above too), then no tool. Undefined when it can run.
 */
export function calculateWhy(hasVectors: boolean, hasTool: boolean): string | undefined {
  if (!hasVectors) {
    return toolpathsStrings.toolpaths.needsSelection;
  }
  return hasTool ? undefined : toolpathsStrings.toolpaths.needsTool;
}

/** Strategies whose forms once overrode the tool's pass depth or stepover. */
const FOLDS_OVERRIDES: ReadonlySet<StrategyName> = new Set([
  'profile',
  'pocket',
  'straightInlay',
  'moulding',
  'rough3d',
  'finish3d',
]);

type ParamsOf<S extends StrategyName> = Extract<ToolpathRequest, { strategy: S }>['params'];

export interface ToolpathFormHandle<S extends StrategyName> {
  tool: Tool | null;
  setTool: (tool: Tool) => void;
  /** Name of the toolpath being edited, or null for a new one. */
  editingName: string | null;
  selectedCount: number;
  /** Vectors are selected, or the Vector Selector picks them. */
  hasVectors: boolean;
  /** The Vector Selection filter; automatic when it is associated. */
  selector: VectorFilter | null;
  setSelector: (selector: VectorFilter | null) => void;
  /** The selected vectors the toolpath would be calculated from. */
  selectedIds: readonly number[];
  /** Drop the toolpath onto the relief model (the params' projectToModel). */
  project: boolean;
  setProject: (project: boolean) => void;
  /** What Calculate waits for while it is off, or undefined when it can run. */
  calculateWhy: string | undefined;
  /** Calculates from the selection; closes the form when it worked. Answers success. */
  calculate: (params: ParamsOf<S>, name: string, options?: CalculateOptions) => Promise<boolean>;
}

export interface CalculateOptions {
  /** Vectors to machine, in order, instead of the selection. */
  ids?: number[];
  /** Leave the form open, for a form that calculates several toolpaths. */
  keepOpen?: boolean;
}

export interface ToolpathFormOptions {
  /** False for strategies that machine the model and need no vectors. */
  requireSelection?: boolean;
  /** The form keeps the Vector Selection filter in its own params. */
  ownSelector?: boolean;
  /** Fills the name field of a new toolpath with the next free name. */
  setName?: (name: string) => void;
}

/**
 * The tool each kind of toolpath was last calculated with: a new form of
 * that kind starts with it, as fields keep their previous setting.
 */
const lastTools = new Map<StrategyName, Tool>();

/** The tool a new `strategy` form starts with, or null for the first. */
export function lastToolFor(strategy: StrategyName): Tool | null {
  return lastTools.get(strategy) ?? null;
}

/** Remembers `tool` as the one `strategy` was last calculated with. */
export function rememberTool(strategy: StrategyName, tool: Tool): void {
  lastTools.set(strategy, tool);
}

export function useToolpathForm<S extends StrategyName>(
  strategy: S,
  load: (params: ParamsOf<S>, name: string, ids: number[]) => void,
  options: ToolpathFormOptions = {},
): ToolpathFormHandle<S> {
  const requireSelection = options.requireSelection ?? true;
  const ownSelector = options.ownSelector === true;
  const setName = options.setName;
  const editing = useUiStore((store) => store.editingToolpath);
  const closeForm = useUiStore((store) => store.closeForm);
  const selectedIds = useDocumentStore((store) => store.selection.ids);
  const [tool, setTool] = useState<Tool | null>(() =>
    editing === null ? lastToolFor(strategy) : null,
  );
  const [editingName, setEditingName] = useState<string | null>(null);
  const [project, setProject] = useState(false);
  const [selector, setSelector] = useState<VectorFilter | null>(null);

  useEffect(() => {
    if (editing !== null || setName === undefined) {
      return;
    }
    const names = useToolpathStore.getState().toolpaths.map((t) => t.name);
    setName(nextToolpathName(TOOLPATH_BASE_NAMES[strategy], names));
    // Once per new toolpath form.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editing, strategy]);

  useEffect(() => {
    if (editing === null) {
      return;
    }
    let live = true;
    void api.cam.get(editing).then((detail) => {
      const request = detail.request;
      if (!live || request === null || request.strategy !== strategy) {
        return;
      }
      setEditingName(detail.name);
      // Pass depth and stepover overrides of older toolpaths become their tool copy.
      const folded = FOLDS_OVERRIDES.has(strategy)
        ? foldToolOverrides(request.tool, request.params)
        : strategy === 'vgroove'
          ? foldMaxPassDepth(request.tool, request.params)
          : { tool: request.tool, params: request.params };
      setTool(folded.tool);
      setProject((request.params as { projectToModel?: boolean }).projectToModel === true);
      if (!ownSelector) {
        setSelector((request.params as { selector?: VectorFilter }).selector ?? null);
      }
      load(folded.params as ParamsOf<S>, detail.name, request.ids);
      // Show which vectors the toolpath machines, as the form edits them.
      void useDocumentStore.getState().select(request.ids);
    });
    return () => {
      live = false;
    };
    // Load once per toolpath opened; `load` only writes form state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editing, strategy]);

  const calculate = useCallback(
    async (params: ParamsOf<S>, name: string, options?: CalculateOptions) => {
      const ids = options?.ids ?? selectedIds;
      const automatic = !ownSelector && isAutomatic(selector);
      if (tool === null || (requireSelection && ids.length === 0 && !automatic)) {
        return false;
      }
      const picked = ownSelector ? params : withSelector(params, selector);
      const request = {
        strategy,
        tool,
        params: project ? { ...picked, projectToModel: true } : picked,
        ids,
        ...(name.trim() === '' ? {} : { name: name.trim() }),
      } as ToolpathRequest;
      const ok = await useToolpathStore
        .getState()
        .calculate(request, editing === null ? undefined : editing);
      if (ok) {
        rememberTool(strategy, tool);
      }
      if (ok && options?.keepOpen !== true) {
        closeForm();
      }
      return ok;
    },
    [
      closeForm,
      editing,
      ownSelector,
      project,
      requireSelection,
      selectedIds,
      selector,
      strategy,
      tool,
    ],
  );

  const hasVectors = selectedIds.length > 0 || (!ownSelector && isAutomatic(selector));
  return {
    tool,
    setTool,
    editingName,
    selectedCount: selectedIds.length,
    hasVectors,
    calculateWhy: calculateWhy(hasVectors, tool !== null),
    selector,
    setSelector,
    selectedIds,
    project,
    setProject,
    calculate,
  };
}
