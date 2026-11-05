import { useEffect, useState, type JSX, type MouseEvent } from 'react';

import { toolEnabled } from '../../lib/enableState';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { runAppCommand } from '../AppShell/AppShell';
import type { EditMode, ToolGroup } from '../LeftPanel/toolGroups';
import { toolIcon } from '../ToolIcons/ToolIcons';
import { whyStrings } from '../../lib/strings/why';
import { whyTip } from '../Tooltip/model';
import { loadToolDescriptions, toolRows, toolTip, type ToolDescriptions } from './model';

import './ToolGroups.scss';

interface ToolGroupsProps {
  groups: readonly ToolGroup[];
}

/** Titled grids of tool icons, as on the Design tab and the Toolpaths panel. */
export function ToolGroups({ groups }: ToolGroupsProps): JSX.Element {
  const openForm = useUiStore((state) => state.openForm);
  const [descriptions, setDescriptions] = useState<ToolDescriptions>({});
  useEffect(() => {
    let live = true;
    void loadToolDescriptions().then((loaded) => {
      if (live) {
        setDescriptions(loaded);
      }
    });
    return () => {
      live = false;
    };
  }, []);
  const activeForm = useUiStore((state) => state.activeForm);
  const editMode = useViewStore((state): EditMode | null =>
    state.activeTool === 'node'
      ? 'node'
      : state.activeTool !== 'select'
        ? null
        : state.transformMode
          ? 'transform'
          : 'select',
  );

  return (
    <div className="tool-groups">
      {groups.map((group) => (
        <section
          aria-labelledby={`tool-group-${group.id}`}
          className="tool-groups__group"
          key={group.id}
        >
          <h2 className="tool-groups__title" id={`tool-group-${group.id}`}>
            {group.title}
          </h2>
          {toolRows(group).map((row) => (
            <div
              aria-labelledby={
                row.caption === undefined ? undefined : `tool-row-${row.tools[0]?.id}`
              }
              className="tool-groups__row"
              key={row.tools[0]?.id}
              role={row.caption === undefined ? undefined : 'group'}
            >
              {row.caption === undefined ? null : (
                <p className="tool-groups__caption" id={`tool-row-${row.tools[0]?.id}`}>
                  {row.caption}
                </p>
              )}
              <div className="tool-groups__grid">
                {row.tools.map((tool) => {
                  const { form, command, shiftCommand, mode } = tool;
                  const run = !toolEnabled(tool)
                    ? undefined
                    : form !== undefined
                      ? () => openForm(form)
                      : command !== undefined
                        ? () => void runAppCommand(command)
                        : undefined;
                  const onClick =
                    run === undefined
                      ? undefined
                      : (event: MouseEvent<HTMLButtonElement>) => {
                          if (event.shiftKey && shiftCommand !== undefined) {
                            void runAppCommand(shiftCommand);
                          } else {
                            run();
                          }
                        };
                  const offWhy = onClick === undefined ? whyStrings.why.notAvailable : undefined;
                  return (
                    <button
                      aria-label={tool.label}
                      aria-pressed={
                        form !== undefined
                          ? activeForm === form
                          : mode !== undefined
                            ? editMode === mode
                            : undefined
                      }
                      className="tool-groups__slot"
                      disabled={offWhy !== undefined}
                      key={tool.id}
                      onClick={onClick}
                      data-tip={whyTip(toolTip(tool, descriptions), offWhy)}
                      data-tool={tool.id}
                      type="button"
                    >
                      {toolIcon(tool.id) ?? tool.short}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}
