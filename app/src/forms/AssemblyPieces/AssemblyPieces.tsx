import type { JSX } from 'react';

import { pieceKey } from '../../bridge/assembly';
import { strings } from '../../lib/formStrings';
import { assemblyMatesStrings } from '../../lib/strings/assemblyMates';
import { isShown, useAssemblyStore } from '../../state/assemblyStore';
import { pieceLabel } from '../AssemblyViewForm/model';
import { anyHidden, isolateNext, showAllWanted } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './AssemblyPieces.scss';

const text = strings.assembly;
const tree = assemblyMatesStrings.tree;

/** An open or shut eye: the piece shows or is hidden. */
function EyeIcon({ open }: { open: boolean }): JSX.Element {
  return (
    <svg aria-hidden="true" className="assembly-pieces__icon" viewBox="0 0 16 16">
      <path d="M1.5 8c1.6-3 3.9-4.5 6.5-4.5S12.9 5 14.5 8c-1.6 3-3.9 4.5-6.5 4.5S3.1 11 1.5 8z" />
      {open ? (
        <circle cx="8" cy="8" r="2.2" />
      ) : (
        <path className="assembly-pieces__strike" d="M2.5 13.5l11-11" />
      )}
    </svg>
  );
}

/** A target: show only this piece. */
function IsolateIcon(): JSX.Element {
  return (
    <svg aria-hidden="true" className="assembly-pieces__icon" viewBox="0 0 16 16">
      <circle cx="8" cy="8" r="5.5" />
      <circle cx="8" cy="8" r="1.8" />
    </svg>
  );
}

/** A push pin: the piece is grounded. */
function PinIcon(): JSX.Element {
  return (
    <svg aria-hidden="true" className="assembly-pieces__icon" viewBox="0 0 16 16">
      <path d="M6 2h4l-.6 4 2.6 2.5H4L6.6 6z" />
      <path d="M8 8.5V14" />
    </svg>
  );
}

/**
 * The pieces as a browser tree: per piece an eye to show or hide it, its
 * name to pick it, Isolate (show only this; again to end it) and Ground (a
 * grounded piece stays put). Hide, show and ground are one undo step each,
 * saved with the job; Isolate is a view state that leaves what is saved
 * hidden alone, so ending it shows what showed before.
 */
export function AssemblyPieces(): JSX.Element {
  const pieces = useAssemblyStore((s) => s.pieces);
  const reliefs = useAssemblyStore((s) => s.reliefs);
  const loading = useAssemblyStore((s) => s.loading);
  const selected = useAssemblyStore((s) => s.selected);
  const select = useAssemblyStore((s) => s.select);
  const setPieceState = useAssemblyStore((s) => s.setPieceState);
  const isolated = useAssemblyStore((s) => s.isolated);
  const isolate = useAssemblyStore((s) => s.isolate);

  return (
    <section aria-labelledby="assembly-pieces-heading" className="assembly-pieces">
      <div className="assembly-pieces__head">
        <h3 className="assembly-pieces__heading" id="assembly-pieces-heading">
          {text.pieces}
        </h3>
        {showAllWanted(pieces, isolated) ? (
          <button
            className="assembly-pieces__show-all"
            onClick={() =>
              anyHidden(pieces) ? void setPieceState('showAll', null) : isolate(null)
            }
            type="button"
            data-tip={tree.showAll}
          >
            <ButtonFace icon="showAll" label={tree.showAll} />
          </button>
        ) : null}
      </div>
      {pieces.length === 0 ? (
        <p className="assembly-pieces__empty">{loading ? text.loading : text.empty}</p>
      ) : (
        <ul aria-labelledby="assembly-pieces-heading" className="assembly-pieces__list">
          {pieces.map((row) => {
            const key = pieceKey(row);
            const name = pieceLabel(row);
            const hidden = row.hidden === true;
            const grounded = row.grounded === true;
            const alone = isolated === key;
            return (
              <li
                className={
                  isShown(row, isolated)
                    ? 'assembly-pieces__row'
                    : 'assembly-pieces__row assembly-pieces__row--hidden'
                }
                key={key}
              >
                <button
                  aria-label={hidden ? tree.show(name) : tree.hide(name)}
                  aria-pressed={!hidden}
                  className="assembly-pieces__toggle"
                  onClick={() => void setPieceState(hidden ? 'show' : 'hide', key)}
                  data-tip={hidden ? tree.show(name) : tree.hide(name)}
                  type="button"
                >
                  <EyeIcon open={!hidden} />
                </button>
                <button
                  aria-pressed={key === selected}
                  className="assembly-pieces__name"
                  onClick={() => select(key === selected ? null : key)}
                  type="button"
                >
                  <span
                    aria-hidden="true"
                    className={
                      reliefs[key] === undefined
                        ? 'assembly-pieces__relief assembly-pieces__relief--loading'
                        : 'assembly-pieces__relief'
                    }
                  />
                  <span className="assembly-pieces__label">{name}</span>
                  {grounded ? <span className="assembly-pieces__tag">{tree.grounded}</span> : null}
                  {hidden ? <span className="assembly-pieces__tag">{tree.hidden}</span> : null}
                </button>
                <button
                  aria-label={alone ? tree.endIsolate : tree.isolate(name)}
                  aria-pressed={alone}
                  className="assembly-pieces__toggle"
                  onClick={() => isolate(isolateNext(isolated, key))}
                  data-tip={alone ? tree.endIsolate : tree.isolate(name)}
                  type="button"
                >
                  <IsolateIcon />
                </button>
                <button
                  aria-label={tree.ground(name)}
                  aria-pressed={grounded}
                  className="assembly-pieces__toggle"
                  onClick={() => void setPieceState(grounded ? 'unground' : 'ground', key)}
                  data-tip={grounded ? tree.unground(name) : tree.ground(name)}
                  type="button"
                >
                  <PinIcon />
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {loading && pieces.length > 0 ? (
        <p aria-live="polite" className="assembly-pieces__status">
          {text.loading}
        </p>
      ) : null}
    </section>
  );
}
