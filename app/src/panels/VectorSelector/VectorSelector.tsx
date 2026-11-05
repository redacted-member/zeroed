import { useEffect, useId, useMemo, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { SelectedVectors, VectorFilter } from '../../bridge/sketch';
import { CheckField } from '../../forms/CheckField/CheckField';
import { NumberField } from '../../forms/NumberField/NumberField';
import {
  filterFrom,
  filterState,
  toggleLayer,
  validateFilter,
  type FilterState,
} from '../../forms/VectorSelectionField/model';
import { strings } from '../../lib/formStrings';
import { unitSuffix } from '../../lib/units';
import { useDocumentStore } from '../../state/documentStore';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

import './VectorSelector.scss';

const text = strings.toolpaths.selector;

interface VectorSelectorProps {
  open: boolean;
  filter: VectorFilter | null;
  /** Called with the filter to keep: the edited one on OK, the old one on Close. */
  onClose: (filter: VectorFilter | null) => void;
  /** Offer "Associate with toolpath" (from a toolpath form). */
  forToolpath: boolean;
}

/** Picks vectors by geometry and layer, selecting them in the 2D view as the filters change. */
export function VectorSelector({
  open,
  filter,
  onClose,
  forToolpath,
}: VectorSelectorProps): JSX.Element {
  const titleId = useId();
  return (
    <ModalDialog
      className="vector-selector"
      closeLabel={text.close}
      onClose={() => onClose(filter)}
      open={open}
      title={text.title}
      titleId={titleId}
    >
      <SelectorBody filter={filter} forToolpath={forToolpath} onClose={onClose} />
    </ModalDialog>
  );
}

interface BodyProps {
  filter: VectorFilter | null;
  onClose: (filter: VectorFilter | null) => void;
  forToolpath: boolean;
}

function SelectorBody({ filter, onClose, forToolpath }: BodyProps): JSX.Element {
  const units = useDocumentStore((store) => store.job.units);
  const layers = useDocumentStore((store) => store.layers);
  const [state, setState] = useState<FilterState>(() => filterState(filter, units));
  const [picked, setPicked] = useState<SelectedVectors | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const errors = useMemo(() => validateFilter(state, units), [state, units]);
  const current = useMemo(() => filterFrom(state, units), [state, units]);
  const okWhy = current === null ? strings.why.fixValues : undefined;
  const key = JSON.stringify(current);
  const unit = unitSuffix(units);

  useEffect(() => {
    if (current === null) {
      return;
    }
    let live = true;
    void api.cam
      .selectVectors(current)
      .then(async (reply) => {
        if (!live) {
          return;
        }
        setPicked(reply);
        setFailure(null);
        await useDocumentStore.getState().select(reply.ids);
      })
      .catch((error: unknown) => {
        if (live) {
          setFailure(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      live = false;
    };
    // The key holds the filter.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const visible = layers.filter((l) => l.visible);
  return (
    <div className="vector-selector__body">
      <p className="vector-selector__counts" role="status">
        {picked === null
          ? ''
          : `${text.objects(picked.ids.length)}  ${text.counts(picked.closed, picked.open)}`}
      </p>
      {failure !== null ? <p className="vector-selector__notice">{failure}</p> : null}
      <fieldset className="vector-selector__group">
        <legend className="vector-selector__legend">{text.geometry}</legend>
        <CheckField
          checked={state.open}
          label={text.openVectors}
          onChange={(open) => setState((s) => ({ ...s, open }))}
        />
        <CheckField
          checked={state.closed}
          disabled={state.onlyCircles}
          label={text.closedVectors}
          onChange={(closed) => setState((s) => ({ ...s, closed }))}
        />
        <CheckField
          checked={state.onlyCircles}
          label={text.onlyCircles}
          onChange={(onlyCircles) => setState((s) => ({ ...s, onlyCircles }))}
        />
        <NumberField
          disabled={!state.onlyCircles}
          error={errors.diameter}
          label={text.diameter}
          onChange={(diameter) => setState((s) => ({ ...s, diameter }))}
          unit={unit}
          value={state.diameter}
        />
        <p className="vector-selector__hint">{text.anyDiameter}</p>
        <NumberField
          disabled={!state.onlyCircles}
          error={errors.tolerance}
          label={text.tolerance}
          onChange={(tolerance) => setState((s) => ({ ...s, tolerance }))}
          unit={unit}
          value={state.tolerance}
        />
      </fieldset>
      <fieldset className="vector-selector__group">
        <legend className="vector-selector__legend">{text.layerFilter}</legend>
        <CheckField
          checked={state.allLayers}
          label={text.allLayers}
          onChange={(allLayers) => setState((s) => ({ ...s, allLayers }))}
        />
        <ul className="vector-selector__layers">
          {visible.map((layer) => (
            <li className="vector-selector__layer" key={layer.id}>
              <CheckField
                checked={!state.allLayers && state.layers.includes(layer.name)}
                label={layer.name}
                onChange={(on) => setState((s) => toggleLayer(s, layer.name, on))}
              />
            </li>
          ))}
        </ul>
      </fieldset>
      {forToolpath ? (
        <fieldset className="vector-selector__group">
          <legend className="vector-selector__legend">{strings.toolpaths.selector.section}</legend>
          <CheckField
            checked={state.associate}
            label={text.associate}
            onChange={(associate) => setState((s) => ({ ...s, associate }))}
          />
          <p className="vector-selector__hint">{text.associateHint}</p>
        </fieldset>
      ) : null}
      <button
        className="vector-selector__apply"
        disabled={okWhy !== undefined}
        onClick={() => onClose(current)}
        type="button"
        data-tip={whyTip(strings.form.ok, okWhy)}
      >
        <ButtonFace icon="formApply" label={strings.form.ok} />
      </button>
    </div>
  );
}
