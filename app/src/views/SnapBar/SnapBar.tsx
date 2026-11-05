import { useEffect, useLayoutEffect, useRef, useState, type JSX } from 'react';

import type { SnapOptions } from '../../bridge/document';
import { strings } from '../../lib/strings';
import { whyStrings } from '../../lib/strings/why';
import { useDocumentStore } from '../../state/documentStore';
import { zoomAllBounds } from '../../lib/zoomAll';
import { usePrefsStore } from '../../state/prefsStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore, type ToolId } from '../../state/viewStore';
import {
  BITMAP_ICON,
  CLEAR_GUIDES_ICON,
  COMPONENT_PREVIEW_ICON,
  TOOLPATH_LINES_ICON,
  TOOLPATH_SOLID_ICON,
  GRID_ICON,
  PAN_ICON,
  RULERS_ICON,
  SNAP_GUIDES_ICON,
  SNAP_SETTINGS_ICON,
  ZOOM_BOX_ICON,
  ZOOM_ALL_ICON,
  ZOOM_JOB_ICON,
  ZOOM_SELECTION_ICON,
} from './icons';
import { whyTip } from '../../shell/Tooltip/model';
import { groupsThatFit, viewToolTip } from './model';

import './SnapBar.scss';

const text = strings.snapBar;

const TOOLS: ReadonlyArray<{ id: ToolId; label: string; icon: JSX.Element; tip: string }> = [
  {
    id: 'zoomBox',
    label: strings.tools.zoomBox,
    icon: ZOOM_BOX_ICON,
    tip: viewToolTip('zoomBox'),
  },
  { id: 'pan', label: strings.tools.pan, icon: PAN_ICON, tip: viewToolTip('pan') },
];

// Snap to geometry, smart snapping and snap to grid are in the document bar
// (views/SnapToggles); snap to guides and the settings stay here.
const SNAP_TOGGLES: ReadonlyArray<{ key: keyof SnapOptions; label: string; icon: JSX.Element }> = [
  { key: 'guides', label: text.guides, icon: SNAP_GUIDES_ICON },
];

interface IconButtonProps {
  label: string;
  /** The tooltip, when it says more than the label (a key). */
  tip?: string;
  icon: JSX.Element;
  onClick: () => void;
  pressed?: boolean;
  /** Why the button is off; the button is off exactly while given. */
  why?: string;
}

function IconButton({ label, tip, icon, onClick, pressed, why }: IconButtonProps): JSX.Element {
  return (
    <button
      aria-label={label}
      aria-pressed={pressed}
      className="snap-bar__button"
      disabled={why !== undefined}
      onClick={onClick}
      data-tip={whyTip(tip ?? label, why)}
      type="button"
    >
      {icon}
    </button>
  );
}

/** Width kept for the More button at the end of a crowded toolbar, px. */
const MORE_WIDTH = 30;

/** The key of the toolbar group a row child is, found by its label. */
function keyOf(
  element: Element,
  groups: ReadonlyArray<{ key: string; label: string }>,
): string | null {
  const label = element.getAttribute('aria-label');
  return groups.find((group) => group.label === label)?.key ?? null;
}

/**
 * The 2D view toolbar: visibility and toolpath drawing toggles, tools, zoom,
 * guide snapping and display toggles, on one row; groups that do not fit move
 * behind a More button at its end. The tile layouts, the drop-downs and the
 * main snapping toggles are in the document bar above (views/ViewArea).
 */
export function SnapBar(): JSX.Element {
  const snap = useViewStore((state) => state.snap);
  const setSnap = useViewStore((state) => state.setSnap);
  const showGrid = useViewStore((state) => state.showGrid);
  const toggleGrid = useViewStore((state) => state.toggleGrid);
  const showRulers = useViewStore((state) => state.showRulers);
  const toggleRulers = useViewStore((state) => state.toggleRulers);
  const activeTool = useViewStore((state) => state.activeTool);
  const setActiveTool = useViewStore((state) => state.setActiveTool);
  const clearGuides = useViewStore((state) => state.clearGuides);
  const zoomToJob = useViewStore((state) => state.zoomToJob);
  const zoomToBounds = useViewStore((state) => state.zoomToBounds);
  const job = useDocumentStore((state) => state.job);
  const selectionBbox = useDocumentStore((state) => state.selection.bbox);
  const openForm = useUiStore((state) => state.openForm);
  const showBitmaps = useViewStore((state) => state.showBitmaps);
  const toggleBitmaps = useViewStore((state) => state.toggleBitmaps);
  const showComponents = useViewStore((state) => state.showComponentPreview);
  const toggleComponents = useViewStore((state) => state.toggleComponentPreview);
  const draw2d = usePrefsStore((state) => state.prefs.toolpaths.draw2dPreviews);
  const solid = usePrefsStore((state) => state.prefs.toolpaths.solidPreviews);
  const updatePrefs = usePrefsStore((state) => state.update);
  const bar = useRef<HTMLDivElement>(null);
  const row = useRef<HTMLDivElement>(null);
  const more = useRef<HTMLButtonElement>(null);
  const overflow = useRef<HTMLDivElement>(null);
  const widths = useRef<Map<string, number>>(new Map());
  const [fit, setFit] = useState(Number.POSITIVE_INFINITY);
  const [moreOpen, setMoreOpen] = useState(false);
  const groups: ReadonlyArray<{ key: string; label: string; content: JSX.Element }> = [
    {
      key: 'viewGroup',
      label: text.viewGroup,
      content: (
        <>
          <IconButton
            icon={COMPONENT_PREVIEW_ICON}
            label={text.componentPreview}
            onClick={toggleComponents}
            pressed={showComponents}
          />
          <IconButton
            icon={BITMAP_ICON}
            label={text.bitmaps}
            onClick={toggleBitmaps}
            pressed={showBitmaps}
          />
          <IconButton
            icon={TOOLPATH_LINES_ICON}
            label={text.toolpaths2d}
            onClick={() => void updatePrefs({ toolpaths: { draw2dPreviews: !draw2d } })}
            pressed={draw2d}
          />
          <IconButton
            why={draw2d ? undefined : whyStrings.why.twoDToolpathsOff}
            icon={TOOLPATH_SOLID_ICON}
            label={text.solidToolpaths}
            onClick={() => void updatePrefs({ toolpaths: { solidPreviews: !solid } })}
            pressed={solid}
          />
        </>
      ),
    },
    {
      key: 'toolsGroup',
      label: text.toolsGroup,
      content: (
        <>
          {TOOLS.map((tool) => (
            <IconButton
              icon={tool.icon}
              key={tool.id}
              label={tool.label}
              onClick={() => setActiveTool(tool.id)}
              pressed={activeTool === tool.id}
              tip={tool.tip}
            />
          ))}
        </>
      ),
    },
    {
      key: 'zoomGroup',
      label: text.zoomGroup,
      content: (
        <>
          <IconButton
            icon={ZOOM_JOB_ICON}
            label={text.zoomToJobHint}
            onClick={() => zoomToJob(job)}
          />
          <IconButton
            icon={ZOOM_ALL_ICON}
            label={text.zoomAllHint}
            onClick={() => zoomToBounds(zoomAllBounds())}
          />
          <IconButton
            why={selectionBbox === null ? whyStrings.why.pickVectors : undefined}
            icon={ZOOM_SELECTION_ICON}
            label={text.zoomToSelectionHint}
            onClick={() => zoomToBounds(selectionBbox)}
          />
        </>
      ),
    },
    {
      key: 'snapGroup',
      label: text.snapGroup,
      content: (
        <>
          {SNAP_TOGGLES.map((toggle) => (
            <IconButton
              icon={toggle.icon}
              key={toggle.key}
              label={toggle.label}
              onClick={() => setSnap({ [toggle.key]: !snap[toggle.key] })}
              pressed={snap[toggle.key]}
            />
          ))}
          <IconButton
            icon={SNAP_SETTINGS_ICON}
            label={text.settingsHint}
            onClick={() => openForm('snapSettings')}
          />
        </>
      ),
    },
    {
      key: 'displayGroup',
      label: text.displayGroup,
      content: (
        <>
          <IconButton
            icon={GRID_ICON}
            label={text.showGrid}
            onClick={toggleGrid}
            pressed={showGrid}
          />
          <IconButton
            icon={RULERS_ICON}
            label={text.showRulers}
            onClick={toggleRulers}
            pressed={showRulers}
          />
          <IconButton icon={CLEAR_GUIDES_ICON} label={text.clearGuides} onClick={clearGuides} />
        </>
      ),
    },
  ];
  const shown = Math.min(fit, groups.length);
  const hidden = groups.slice(shown);

  // Groups showing in the row are measured after every render (a level
  // picker comes and goes), then the row keeps as many as fit.
  useLayoutEffect(() => {
    const measure = () => {
      const rowElement = row.current;
      const barElement = bar.current;
      if (rowElement === null || barElement === null) {
        return;
      }
      for (const child of Array.from(rowElement.children)) {
        const key = keyOf(child, groups);
        if (key !== null) {
          widths.current.set(key, (child as HTMLElement).offsetWidth);
        }
      }
      const style = getComputedStyle(barElement);
      const available =
        barElement.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      const next = groupsThatFit(
        groups.map((group) => widths.current.get(group.key)),
        available,
        MORE_WIDTH,
      );
      setFit((current) => (current === next ? current : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (bar.current !== null) {
      observer.observe(bar.current);
    }
    return () => observer.disconnect();
  });

  useEffect(() => {
    if (!moreOpen) {
      return;
    }
    const inside = (target: EventTarget | null) =>
      target instanceof Node &&
      (more.current?.contains(target) === true || overflow.current?.contains(target) === true);
    const onPointerDown = (event: PointerEvent) => {
      if (!inside(event.target)) {
        setMoreOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setMoreOpen(false);
        more.current?.focus();
      }
    };
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [moreOpen]);

  return (
    <div aria-label={text.toolbar} className="snap-bar" ref={bar} role="toolbar">
      <div className="snap-bar__row" ref={row}>
        {groups.slice(0, shown).map((group) => (
          <div aria-label={group.label} className="snap-bar__group" key={group.key} role="group">
            {group.content}
          </div>
        ))}
      </div>
      {hidden.length === 0 ? null : (
        <button
          aria-controls="snap-bar-more"
          aria-expanded={moreOpen}
          aria-haspopup="true"
          aria-label={text.more}
          className="snap-bar__more"
          onClick={() => setMoreOpen((open) => !open)}
          ref={more}
          data-tip={text.more}
          type="button"
        >
          <span aria-hidden="true">{'\u00bb'}</span>
        </button>
      )}
      {moreOpen && hidden.length > 0 ? (
        <div className="snap-bar__overflow" id="snap-bar-more" ref={overflow}>
          {hidden.map((group) => (
            <div aria-label={group.label} className="snap-bar__group" key={group.key} role="group">
              {group.content}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
