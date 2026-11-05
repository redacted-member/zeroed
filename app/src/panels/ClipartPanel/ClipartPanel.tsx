import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type JSX,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { createPortal } from 'react-dom';

import { api } from '../../bridge/formApi';
import type { ClipItem } from '../../bridge/clipart';
import { strings } from '../../lib/formStrings';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';
import { drawIcon } from '../../shell/ToolIcons/ToolIcons';
import { useDocumentStore } from '../../state/documentStore';
import { dropOnFace, dropPointAt, isDrag } from './drop';
import {
  ALL_CATEGORIES,
  categoryCounts,
  categoryTree,
  filterClips,
  type CategoryNode,
  type ThumbSize,
  jobCentre,
  scaleFromPercent,
} from './model';
import { useClipartStore } from './store';

import './ClipartPanel.scss';

const text = strings.clipart;

// Icons of the tab's buttons, as shell/ToolIcons specs on a 24 grid.
const ICON_ALL = 'M4 6h16M4 12h16M4 18h16';
const ICON_CLOSED = 't:M3 6h6l2 2h10v11H3Z;M10 11l3 2.5-3 2.5';
const ICON_OPEN = 't:M3 6h6l2 2h10v11H3Z;M9 12l3 3 3-3';
const ICON_FOLDER = 't:M3 6h6l2 2h10v11H3Z';
const ICON_SMALL =
  't:r4 4 4 4;t:r10 4 4 4;t:r16 4 4 4;t:r4 10 4 4;t:r10 10 4 4;t:r16 10 4 4;t:r4 16 4 4;t:r10 16 4 4;t:r16 16 4 4';
const ICON_MEDIUM = 't:r4 4 7 7;t:r13 4 7 7;t:r4 13 7 7;t:r13 13 7 7';
const ICON_LARGE = 't:r4 4 16 16';
const ICON_INSERT = 't:r3 12 18 9;M12 3v11M8 10l4 4 4-4';
const ICON_RESCAN = 'M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5';

/** Most tiles shown at once; a search narrows the rest. */
const MAX_TILES = 300;

/**
 * Object URLs of clip thumbnails, fetched one at a time for the paths asked
 * for and revoked when the panel goes away. An empty string marks a clip
 * whose thumbnail could not be drawn.
 */
function useThumbnails(paths: readonly string[]): ReadonlyMap<string, string> {
  const cache = useRef(new Map<string, string>());
  const pending = useRef(new Set<string>());
  const mounted = useRef(false);
  const [urls, setUrls] = useState<ReadonlyMap<string, string>>(() => new Map());

  useEffect(() => {
    mounted.current = true;
    const owned = cache.current;
    return () => {
      mounted.current = false;
      for (const url of owned.values()) {
        if (url !== '') {
          URL.revokeObjectURL(url);
        }
      }
      owned.clear();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const wanted = paths.filter((p) => !cache.current.has(p) && !pending.current.has(p));
    const load = async () => {
      for (const path of wanted) {
        if (cancelled) {
          return;
        }
        pending.current.add(path);
        try {
          const blob = await api.clipart.thumbnail(path);
          const url = URL.createObjectURL(blob);
          if (mounted.current) {
            cache.current.set(path, url);
          } else {
            URL.revokeObjectURL(url);
          }
        } catch {
          if (mounted.current) {
            cache.current.set(path, '');
          }
        } finally {
          pending.current.delete(path);
        }
        if (mounted.current) {
          setUrls(new Map(cache.current));
        }
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [paths]);

  return urls;
}

export function ClipartPanel(): JSX.Element {
  const searchId = useId();
  const scaleId = useId();
  const categoriesId = useId();
  const items = useClipartStore((s) => s.items);
  const categories = useClipartStore((s) => s.categories);
  const loading = useClipartStore((s) => s.loading);
  const loaded = useClipartStore((s) => s.loaded);
  const error = useClipartStore((s) => s.error);
  const status = useClipartStore((s) => s.status);
  const userDir = useClipartStore((s) => s.userDir);
  const category = useClipartStore((s) => s.category);
  const query = useClipartStore((s) => s.query);
  const scaleText = useClipartStore((s) => s.scaleText);
  const thumbSize = useClipartStore((s) => s.thumbSize);
  const expanded = useClipartStore((s) => s.expanded);
  const store = useClipartStore.getState;
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    if (!useClipartStore.getState().loaded) {
      void useClipartStore.getState().scan();
    }
  }, []);

  const counts = useMemo(() => categoryCounts(items), [items]);
  const tree = useMemo(() => categoryTree(categories, counts), [categories, counts]);
  const shown = useMemo(() => filterClips(items, category, query), [items, category, query]);
  const tiles = useMemo(() => shown.slice(0, MAX_TILES), [shown]);
  const paths = useMemo(() => tiles.map((t) => t.path), [tiles]);
  const thumbnails = useThumbnails(paths);
  const scaleValid = scaleFromPercent(scaleText) !== null;
  const selectedItem = tiles.find((t) => t.path === selected) ?? null;
  const insertWhy = firstWhy(
    [selectedItem === null, strings.why.pickClipart],
    [!scaleValid, strings.why.fixValues],
  );
  const rescanWhy = loading ? strings.why.busy : undefined;

  const insertAtCentre = (item: ClipItem) => {
    const centre = jobCentre(useDocumentStore.getState().job);
    void store().insert(item, centre.x, centre.y);
  };

  /** The press on a tile that may become a drag onto a view. */
  const pressRef = useRef<{
    pointerId: number;
    item: ClipItem;
    from: { x: number; y: number };
    dragging: boolean;
  } | null>(null);
  /** Set when a drag ends, so the click that follows does not select. */
  const dragged = useRef(false);
  const [ghost, setGhost] = useState<{ path: string; x: number; y: number } | null>(null);

  const startDrag = (event: ReactPointerEvent<HTMLButtonElement>, item: ClipItem) => {
    if (event.button !== 0) {
      return;
    }
    // No text selection while dragging; the tile still takes the focus.
    event.preventDefault();
    event.currentTarget.focus();
    pressRef.current = {
      pointerId: event.pointerId,
      item,
      from: { x: event.clientX, y: event.clientY },
      dragging: false,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const moveDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const press = pressRef.current;
    if (press === null || press.pointerId !== event.pointerId) {
      return;
    }
    const at = { x: event.clientX, y: event.clientY };
    if (!press.dragging && !isDrag(press.from, at)) {
      return;
    }
    press.dragging = true;
    setGhost({ path: press.item.path, x: at.x, y: at.y });
  };

  const cancelDrag = () => {
    pressRef.current = null;
    setGhost(null);
  };

  const endDrag = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const press = pressRef.current;
    if (press === null || press.pointerId !== event.pointerId) {
      return;
    }
    cancelDrag();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (!press.dragging) {
      return;
    }
    dragged.current = true;
    if (dropOnFace(event.clientX, event.clientY, press.item.path)) {
      return;
    }
    const at = dropPointAt(event.clientX, event.clientY);
    if (at !== null) {
      void store().insert(press.item, at.x, at.y);
    }
  };

  const ghostUrl = ghost === null ? undefined : thumbnails.get(ghost.path);

  const folder = (node: CategoryNode, depth: number): JSX.Element => {
    const open = expanded.includes(node.path);
    const hasChildren = node.children.length > 0;
    return (
      <li key={node.path}>
        <button
          aria-current={category === node.path ? 'true' : undefined}
          aria-expanded={hasChildren ? open : undefined}
          className={`clipart-panel__category clipart-panel__category--depth-${Math.min(depth, 3)}`}
          onClick={() => store().chooseFolder(node.path)}
          type="button"
        >
          <span aria-hidden="true" className="clipart-panel__category-icon">
            {drawIcon(hasChildren ? (open ? ICON_OPEN : ICON_CLOSED) : ICON_FOLDER)}
          </span>
          <span className="clipart-panel__category-name">{node.name}</span>
          <span className="clipart-panel__category-count">{node.count}</span>
        </button>
        {hasChildren && open ? (
          <ul className="clipart-panel__category-list">
            {node.children.map((child) => folder(child, depth + 1))}
          </ul>
        ) : null}
      </li>
    );
  };

  const sizeButton = (size: ThumbSize, icon: string, label: string, title: string) => (
    <button
      aria-pressed={thumbSize === size}
      className="clipart-panel__size"
      onClick={() => store().setThumbSize(size)}
      data-tip={title}
      type="button"
    >
      <span aria-hidden="true" className="clipart-panel__button-icon">
        {drawIcon(icon)}
      </span>
      <span className="clipart-panel__button-label">{label}</span>
    </button>
  );

  return (
    <div className="clipart-panel">
      <div className="clipart-panel__controls">
        <label className="clipart-panel__label" htmlFor={searchId}>
          {text.search}
        </label>
        <input
          className="clipart-panel__input"
          id={searchId}
          onChange={(event) => store().setQuery(event.target.value)}
          placeholder={text.searchPlaceholder}
          type="search"
          value={query}
        />
        <label className="clipart-panel__label" htmlFor={scaleId}>
          {text.scale}
        </label>
        <input
          aria-invalid={!scaleValid}
          className="clipart-panel__input clipart-panel__input--scale"
          id={scaleId}
          inputMode="decimal"
          onChange={(event) => store().setScaleText(event.target.value)}
          type="text"
          value={scaleText}
        />
        <span className="clipart-panel__label">{text.thumbnails}</span>
        <div aria-label={text.thumbnails} className="clipart-panel__sizes" role="group">
          {sizeButton('small', ICON_SMALL, text.small, text.smallTitle)}
          {sizeButton('medium', ICON_MEDIUM, text.medium, text.mediumTitle)}
          {sizeButton('large', ICON_LARGE, text.large, text.largeTitle)}
        </div>
      </div>

      <nav aria-labelledby={categoriesId} className="clipart-panel__categories">
        <h2 className="clipart-panel__heading" id={categoriesId}>
          {text.categories}
        </h2>
        <ul className="clipart-panel__category-list">
          <li>
            <button
              aria-current={category === ALL_CATEGORIES ? 'true' : undefined}
              className="clipart-panel__category clipart-panel__category--depth-0"
              onClick={() => store().setCategory(ALL_CATEGORIES)}
              type="button"
            >
              <span aria-hidden="true" className="clipart-panel__category-icon">
                {drawIcon(ICON_ALL)}
              </span>
              <span className="clipart-panel__category-name">{text.all}</span>
              <span className="clipart-panel__category-count">{items.length}</span>
            </button>
          </li>
          {tree.map((node) => folder(node, 0))}
        </ul>
      </nav>

      <section aria-label={text.clips} className="clipart-panel__library">
        {loading && !loaded ? <p className="clipart-panel__note">{text.loading}</p> : null}
        {loaded && tiles.length === 0 ? (
          <p className="clipart-panel__note">{query === '' ? text.empty : text.noMatch}</p>
        ) : null}
        <ul className={`clipart-panel__grid clipart-panel__grid--${thumbSize}`}>
          {tiles.map((item) => {
            const url = thumbnails.get(item.path);
            return (
              <li className="clipart-panel__cell" key={item.path}>
                <button
                  aria-pressed={selected === item.path}
                  className={
                    ghost?.path === item.path
                      ? 'clipart-panel__tile clipart-panel__tile--dragging'
                      : 'clipart-panel__tile'
                  }
                  draggable={false}
                  onClick={() => {
                    if (!dragged.current) {
                      setSelected(item.path);
                    }
                    dragged.current = false;
                  }}
                  onDoubleClick={() => insertAtCentre(item)}
                  onPointerCancel={cancelDrag}
                  onPointerDown={(event) => startDrag(event, item)}
                  onPointerMove={moveDrag}
                  onPointerUp={endDrag}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      insertAtCentre(item);
                    }
                  }}
                  data-tip={text.tileHint(item.name, item.category)}
                  type="button"
                >
                  <span className="clipart-panel__thumb">
                    {url === undefined ? (
                      <span className="clipart-panel__thumb-wait" />
                    ) : url === '' ? (
                      <span className="clipart-panel__thumb-missing">{text.noPreview}</span>
                    ) : (
                      <img alt="" className="clipart-panel__image" draggable={false} src={url} />
                    )}
                  </span>
                  <span className="clipart-panel__name">{item.name}</span>
                  <span className="clipart-panel__kind">
                    {item.kind === 'relief' ? text.relief : text.vector}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
        {shown.length > tiles.length ? (
          <p className="clipart-panel__note">{text.more(shown.length - tiles.length)}</p>
        ) : null}
      </section>

      <div className="clipart-panel__footer">
        <p className="clipart-panel__hint">{text.dragHint}</p>
        {error !== null ? (
          <p className="clipart-panel__error" role="alert">
            {error}
          </p>
        ) : null}
        {status !== null && error === null ? (
          <p className="clipart-panel__status" role="status">
            {status}
          </p>
        ) : null}
        <div className="clipart-panel__buttons">
          <button
            className="clipart-panel__button"
            disabled={insertWhy !== undefined}
            onClick={() => selectedItem !== null && insertAtCentre(selectedItem)}
            data-tip={whyTip(text.insertTitle, insertWhy)}
            type="button"
          >
            <span aria-hidden="true" className="clipart-panel__button-icon">
              {drawIcon(ICON_INSERT)}
            </span>
            <span className="clipart-panel__button-label">{text.insert}</span>
          </button>
          <button
            className="clipart-panel__button"
            disabled={rescanWhy !== undefined}
            onClick={() => void store().scan()}
            data-tip={whyTip(text.rescanTitle, rescanWhy)}
            type="button"
          >
            <span aria-hidden="true" className="clipart-panel__button-icon">
              {drawIcon(ICON_RESCAN)}
            </span>
            <span className="clipart-panel__button-label">{text.rescan}</span>
          </button>
        </div>
        {userDir !== null ? <p className="clipart-panel__hint">{text.folder(userDir)}</p> : null}
      </div>
      {ghost !== null
        ? createPortal(
            <div
              aria-hidden="true"
              className="clipart-panel__ghost"
              style={{ left: ghost.x, top: ghost.y }}
            >
              {ghostUrl !== undefined && ghostUrl !== '' ? (
                <img alt="" className="clipart-panel__image" draggable={false} src={ghostUrl} />
              ) : null}
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
