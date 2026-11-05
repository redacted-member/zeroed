import { useEffect, useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { FilletKind, Point } from '../../bridge/document';
import { CHECK_CURSOR, REMOVE_CURSOR } from '../../lib/cursors';
import { HoverProbe } from '../../lib/hoverProbe';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDrawController } from '../../lib/useDrawController';
import { useFormState } from '../../lib/useFormState';
import { useDocumentStore } from '../../state/documentStore';
import type { DrawController } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { useViewStore } from '../../state/viewStore';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { NumberField } from '../NumberField/NumberField';
import {
  convertFilletUnits,
  FILLET_KINDS,
  filletRadius,
  initialFilletState,
  pickTolerance,
  radiusError,
} from './model';

import './FilletForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { HelperRow } from '../HelperRow/HelperRow';
import { filletPicture } from '../HelperPicture/pictures/drawing';

const text = strings.editObjects.fillet;

const KINDS: ReadonlyArray<Choice<FilletKind>> = FILLET_KINDS.map((value) => ({
  value,
  label: text[value],
}));

export function FilletForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const { state, setState, stateRef, units, unitsRef } = useFormState(
    initialFilletState,
    convertFilletUnits,
  );
  const [missed, setMissed] = useState(false);
  const error = useMemo(() => radiusError(state, units), [state, units]);

  const hoverRef = useRef<HoverProbe | null>(null);
  useEffect(() => {
    const tolerance = () => {
      const view = useViewStore.getState();
      return pickTolerance(view.snapRadiusPx, view.camera.pxPerMm);
    };
    // The cursor shows what a click would do: a check mark over a corner
    // that takes the fillet, a cross over a fillet a click takes out.
    hoverRef.current = new HoverProbe(async (at) => {
      const radius = filletRadius(stateRef.current, unitsRef.current);
      const { ids } = await api.entities.hitTest({ x: at.x, y: at.y, tolerance: tolerance() });
      const [id] = ids;
      if (radius === null || id === undefined) {
        return 'crosshair';
      }
      const { action } = await api.ops.filletProbe(id, at.x, at.y, radius, stateRef.current.kind);
      return action === 'fillet' ? CHECK_CURSOR : action === 'remove' ? REMOVE_CURSOR : 'crosshair';
    }, 'crosshair');
    return () => {
      hoverRef.current = null;
    };
  }, [stateRef, unitsRef]);

  const controller = useMemo<DrawController>(() => {
    const tolerance = () => {
      const view = useViewStore.getState();
      return pickTolerance(view.snapRadiusPx, view.camera.pxPerMm);
    };
    const filletHere = (at: Point, radius: number) => {
      void useDocumentStore
        .getState()
        .filletAt(at.x, at.y, tolerance(), radius, stateRef.current.kind)
        .then((hit) => {
          setMissed(!hit);
          hoverRef.current?.reset();
        });
    };
    return {
      get cursor() {
        return hoverRef.current?.cursor ?? 'crosshair';
      },
      pointerDown: (p) => {
        const radius = filletRadius(stateRef.current, unitsRef.current);
        if (radius !== null) {
          filletHere({ x: p.x, y: p.y }, radius);
        }
      },
      pointerMove: (p) => hoverRef.current?.move({ x: p.rawX, y: p.rawY }),
      pointerUp: () => undefined,
      // A radius typed while hovering, then Enter: set it and fillet there.
      quickEntry: (entry, cursor) => {
        const [value] = entry.values;
        if (value === undefined || !(value > 0)) {
          return false;
        }
        const units = unitsRef.current;
        const text = String(value);
        setState((current) => ({ ...current, radius: text }));
        const radius = filletRadius({ ...stateRef.current, radius: text }, units);
        if (radius !== null) {
          filletHere(cursor, radius);
        }
        return true;
      },
      cancel: () => hoverRef.current?.reset(),
    };
  }, [setState, stateRef, unitsRef]);
  useDrawController(controller);

  return (
    <div className="fillet-form">
      <p className="fillet-form__hint">{text.hint}</p>
      {missed ? <p className="fillet-form__notice">{text.missed}</p> : null}
      <fieldset className="fillet-form__group">
        <legend className="fillet-form__legend">{text.title}</legend>
        <NumberField
          error={error}
          label={text.radius}
          onChange={(radius) => setState((current) => ({ ...current, radius }))}
          unit={unitSuffix(units)}
          value={state.radius}
        />
        <HelperRow picture={<HelperPicture picture={filletPicture(state.kind)} />}>
          <ChoiceField
            label={text.type}
            onChange={(kind) => setState((current) => ({ ...current, kind }))}
            options={KINDS}
            value={state.kind}
          />
        </HelperRow>
      </fieldset>
      <FormButtons onClose={closeForm} onPrimary={closeForm} primaryLabel={strings.form.ok} />
    </div>
  );
}
