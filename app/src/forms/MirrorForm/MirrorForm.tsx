import { useEffect, useMemo, useState, type JSX } from 'react';

import { api } from '../../bridge/api';
import type { Point } from '../../bridge/document';
import { strings } from '../../lib/formStrings';
import { selectionFrame } from '../../lib/transformMath';
import { useDocumentStore } from '../../state/documentStore';
import { useUiStore } from '../../state/uiStore';
import { jobBounds } from '../../views/View2D/camera';
import { FormButtons } from '../FormButtons/FormButtons';
import { IconButton } from '../IconButton/IconButton';
import {
  MIRROR_BUTTONS,
  initialMirrorState,
  mirrorAcross,
  mirrorLine,
  mirrorMatrixFor,
  type MirrorAction,
} from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './MirrorForm.scss';

/** The mirror line: the last selected object when it is one straight line and others are selected too. */
function useMirrorLine(): { id: number; line: [Point, Point] } | null {
  const ids = useDocumentStore((store) => store.selection.ids);
  const last = ids.length >= 2 ? ids[ids.length - 1] : undefined;
  const [found, setFound] = useState<{ id: number; line: [Point, Point] } | null>(null);
  useEffect(() => {
    if (last === undefined) {
      return;
    }
    let live = true;
    void api.entities
      .get(last)
      .then((entity) => {
        const line = entity.kind === 'path' ? mirrorLine(entity.path) : null;
        if (live) {
          setFound(line === null ? null : { id: last, line });
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [last]);
  return last === undefined || found?.id !== last ? null : found;
}

export function MirrorForm(): JSX.Element {
  const selection = useDocumentStore((store) => store.selection);
  const job = useDocumentStore((store) => store.job);
  // Use Rotated Bounds applies to one object only.
  const single = selection.ids.length === 1;
  const transformSelection = useDocumentStore((store) => store.transformSelection);
  const transformSelectionCopy = useDocumentStore((store) => store.transformSelectionCopy);
  const closeForm = useUiStore((store) => store.closeForm);
  const text = strings.transform.mirror;
  const [state, setState] = useState(initialMirrorState);
  const bbox = selection.bbox;
  const frame = useMemo(() => selectionFrame(selection), [selection]);
  const line = useMirrorLine();
  const lineWhy = line === null ? text.lineHint : undefined;
  const material = useMemo(() => {
    const b = jobBounds(job);
    return [b.minX, b.minY, b.maxX, b.maxY] as [number, number, number, number];
  }, [job]);

  const apply = async (matrix: ReturnType<typeof mirrorMatrixFor>, ids?: number[]) => {
    if (matrix === null) {
      return;
    }
    if (state.copy) {
      await transformSelectionCopy(matrix, ids);
    } else {
      await transformSelection(matrix, ids);
    }
  };

  const flip = (mode: MirrorAction) =>
    apply(
      mirrorMatrixFor(bbox, mode, {
        frame,
        rotatedBounds: state.rotatedBounds && single,
        material,
      }),
    );

  const flipAcrossLine = () => {
    if (line !== null) {
      void apply(
        mirrorAcross(line.line),
        selection.ids.filter((id) => id !== line.id),
      );
    }
  };

  return (
    <div className="mirror-form">
      {bbox === null ? (
        <p className="mirror-form__notice">{strings.transform.noSelection}</p>
      ) : null}
      <fieldset className="mirror-form__group">
        <legend className="mirror-form__legend">{text.title}</legend>
        <div className="mirror-form__grid">
          {MIRROR_BUTTONS.map((button) => (
            <IconButton
              why={bbox === null ? strings.transform.noSelection : undefined}
              icon={button.icon}
              key={button.mode}
              label={button.short}
              onClick={() => void flip(button.mode)}
              title={button.label}
            />
          ))}
        </div>
        <button
          className="mirror-form__button"
          disabled={lineWhy !== undefined}
          onClick={flipAcrossLine}
          type="button"
          data-tip={whyTip(text.acrossLine, lineWhy)}
        >
          <ButtonFace icon="mirror" label={text.acrossLine} />
        </button>
        <p className="mirror-form__notice">{text.lineHint}</p>
        <label className="mirror-form__choice">
          <input
            checked={state.copy}
            onChange={(event) => setState((s) => ({ ...s, copy: event.target.checked }))}
            type="checkbox"
          />
          {text.copy}
        </label>
        <label className="mirror-form__choice">
          <input
            checked={state.rotatedBounds}
            disabled={!single}
            onChange={(event) => setState((s) => ({ ...s, rotatedBounds: event.target.checked }))}
            type="checkbox"
          />
          {text.rotatedBounds}
        </label>
      </fieldset>
      <FormButtons onClose={closeForm} />
    </div>
  );
}
