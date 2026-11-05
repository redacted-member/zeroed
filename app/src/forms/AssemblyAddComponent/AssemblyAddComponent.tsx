import { useCallback, useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { strings } from '../../lib/formStrings';
import { toolIcon, useExtraIcons } from '../../shell/ToolIcons/ToolIcons';
import { useAssemblyStore } from '../../state/assemblyStore';
import { pickedComponents, useModelStore } from '../../state/modelStore';
import { IconButton } from '../IconButton/IconButton';
import { componentOptions, noComponentNote } from '../AssemblyViewForm/model';
import { filterComponents, staleThumbnails, type ThumbnailWanted } from './model';

import './AssemblyAddComponent.scss';

const text = strings.assembly;
/** Thumbnail size asked of the kernel, pixels (drawn at half for sharpness). */
const THUMBNAIL_PIXELS = 96;

/**
 * Each listed component's thumbnail, loaded while the list shows, one
 * request at a time: those not loaded yet, then again whenever a
 * component's shape changes (its look does, on the document revision);
 * the old picture shows until the new one comes.
 */
function useThumbnails(wanted: readonly ThumbnailWanted[]): ReadonlyMap<number, string> {
  const [thumbnails, setThumbnails] = useState<ReadonlyMap<number, string>>(() => new Map());
  const urls = useRef(new Map<number, string>());
  const looks = useRef(new Map<number, string>());
  const latest = useRef<readonly ThumbnailWanted[]>([]);
  const running = useRef(false);
  const alive = useRef(true);
  const key = wanted.map((w) => `${w.id}:${w.look ?? ''}`).join(',');

  // First, so a remount marks the hook alive before the list asks again.
  useEffect(() => {
    alive.current = true;
    const held = urls.current;
    const made = looks.current;
    return () => {
      alive.current = false;
      for (const url of held.values()) {
        if (url !== '') {
          URL.revokeObjectURL(url);
        }
      }
      held.clear();
      made.clear();
    };
  }, []);

  const pump = useCallback(async () => {
    if (running.current) {
      return;
    }
    running.current = true;
    try {
      for (;;) {
        const next = latest.current.find((w) => staleThumbnails([w], looks.current).length > 0);
        if (next === undefined || !alive.current) {
          return;
        }
        const look = next.look ?? '';
        let url = '';
        try {
          url = URL.createObjectURL(
            await api.assembly.componentThumbnail(next.id, THUMBNAIL_PIXELS),
          );
        } catch {
          // A component that makes no piece shows no thumbnail.
        }
        if (!alive.current) {
          if (url !== '') {
            URL.revokeObjectURL(url);
          }
          return;
        }
        const old = urls.current.get(next.id);
        if (old !== undefined && old !== '') {
          URL.revokeObjectURL(old);
        }
        urls.current.set(next.id, url);
        looks.current.set(next.id, look);
        setThumbnails(new Map(urls.current));
      }
    } finally {
      running.current = false;
    }
  }, []);

  useEffect(() => {
    latest.current =
      key === ''
        ? []
        : key.split(',').map((entry) => {
            const [id = '', look = ''] = entry.split(':');
            return { id: Number(id), look };
          });
    void pump();
  }, [key, pump]);

  return thumbnails;
}

/**
 * Add Component: the components that are not pieces yet, each with its
 * thumbnail and name, a search, Add (stands it beside the pieces) and
 * Place (its ghost follows the pointer in the 3D view until a click sets
 * it down; Esc cancels). A line says the other way in: drag a component
 * from the Components tab onto the 3D view.
 */
export function AssemblyAddComponent(): JSX.Element {
  useExtraIcons();
  const components = useAssemblyStore((s) => s.components);
  const addComponents = useAssemblyStore((s) => s.addComponents);
  const placing = useAssemblyStore((s) => s.placing);
  const startPlacing = useAssemblyStore((s) => s.startPlacing);
  const cancelPlacing = useAssemblyStore((s) => s.cancelPlacing);
  const selectedId = useModelStore((s) => s.selectedId);
  const pickedIds = useModelStore((s) => s.picked);
  const jobComponents = useModelStore((s) => s.components.length);
  const [query, setQuery] = useState('');
  const picked = pickedComponents({ selectedId, picked: pickedIds });
  const options = useMemo(() => componentOptions(components), [components]);
  const shown = filterComponents(options, query, picked);
  const wanted = useMemo(
    () => components.map((c) => ({ id: c.id, look: c.look ?? '' })),
    [components],
  );
  const thumbnails = useThumbnails(wanted);
  const waiting = placing !== null && !placing.dragging ? placing : null;
  const waitingName =
    waiting === null ? null : (options.find((o) => o.id === waiting.component)?.label ?? null);

  return (
    <fieldset className="assembly-view__section">
      <legend className="assembly-view__heading">{text.addComponent}</legend>
      <p className="assembly-add__ways">
        <span aria-hidden="true" className="assembly-add__ways-icon">
          {toolIcon('pieceDrag')}
        </span>
        {text.addComponentWays}
      </p>
      {options.length === 0 ? (
        <p className="assembly-view__empty">{noComponentNote(jobComponents)}</p>
      ) : (
        <>
          <input
            aria-label={text.searchComponents}
            className="assembly-add__search"
            onChange={(event) => setQuery(event.target.value)}
            placeholder={text.searchComponents}
            type="search"
            value={query}
          />
          {shown.length === 0 ? (
            <p className="assembly-view__empty">{text.noComponentMatch(query.trim())}</p>
          ) : (
            <ul aria-label={text.addComponent} className="assembly-add__list">
              {shown.map((option) => {
                const url = thumbnails.get(option.id);
                const isPlacing = waiting?.component === option.id;
                return (
                  <li
                    className={
                      picked.includes(option.id)
                        ? 'assembly-add__row assembly-add__row--picked'
                        : 'assembly-add__row'
                    }
                    key={option.id}
                  >
                    <span className="assembly-add__thumb">
                      {url === undefined || url === '' ? null : (
                        <img alt="" className="assembly-add__image" draggable={false} src={url} />
                      )}
                    </span>
                    <span className="assembly-add__name">{option.label}</span>
                    <IconButton
                      description={text.addBesideDescription}
                      icon="pieceAdd"
                      label={text.add}
                      onClick={() => void addComponents([option.id], { beside: true })}
                      title={text.addBeside(option.label)}
                    />
                    <IconButton
                      description={text.placeDescription}
                      icon="piecePlace"
                      label={text.place}
                      onClick={() => (isPlacing ? cancelPlacing() : startPlacing(option.id, false))}
                      pressed={isPlacing}
                      title={text.placeTitle(option.label)}
                    />
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}
      {waiting === null ? null : (
        <div className="assembly-add__placing">
          <p aria-live="polite" className="assembly-view__status">
            {text.placing(waitingName ?? text.componentPiece(waiting.component))}
          </p>
          <IconButton
            icon="pieceCancel"
            label={text.cancel}
            onClick={cancelPlacing}
            title={text.cancelPlace}
          />
        </div>
      )}
    </fieldset>
  );
}
