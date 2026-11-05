import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { api } from '../../bridge/api';

import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { useTraceViewStore } from '../../state/traceViewStore';
import { useUiStore } from '../../state/uiStore';
import { CheckField } from '../CheckField/CheckField';
import { ChoiceField, type Choice } from '../ChoiceField/ChoiceField';
import { FormButtons } from '../FormButtons/FormButtons';
import { SliderField } from '../SliderField/SliderField';
import { selectedBitmap } from '../TraceBitmapForm/model';
import {
  editPixels,
  initialPictureState,
  isNeutral,
  pictureRequest,
  type PictureBorder,
  type PictureState,
} from './model';
import { firstWhy } from '../../shell/Tooltip/model';

import './EditPictureForm.scss';

const text = strings.picture.edit;

const BORDERS: ReadonlyArray<Choice<PictureBorder>> = [
  { value: 'none', label: text.borderNone },
  { value: 'rectangle', label: text.borderRectangle },
  { value: 'oval', label: text.borderOval },
];

/** Edit Picture: tone, invert, greyscale and a faded border for the selected bitmap. */
export function EditPictureForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const editPicture = useDocumentStore((store) => store.editPicture);
  const bitmapId = useDocumentStore((store) => selectedBitmap(store.entities, store.selection.ids));
  const lastError = useDocumentStore((store) => store.lastError);
  const [state, setState] = useState<PictureState>(initialPictureState);
  const [busy, setBusy] = useState(false);
  const request = useMemo(() => pictureRequest(state, bitmapId), [state, bitmapId]);

  // The picture follows the sliders in the 2D view; Apply makes it so.
  const source = useRef<{ bitmap: number; imageId: number; pixels: ImageData } | null>(null);
  useEffect(() => {
    if (bitmapId === null) {
      return undefined;
    }
    let live = true;
    const timer = window.setTimeout(() => {
      void (async () => {
        let base = source.current;
        if (base === null || base.bitmap !== bitmapId) {
          const entity = await api.entities.get(bitmapId);
          if (entity.kind !== 'bitmap') {
            return;
          }
          const decoded = await createImageBitmap(await api.bitmap.pixels(entity.imageId));
          const canvas = new OffscreenCanvas(decoded.width, decoded.height);
          const context = canvas.getContext('2d');
          if (context === null) {
            return;
          }
          context.drawImage(decoded, 0, 0);
          base = {
            bitmap: bitmapId,
            imageId: entity.imageId,
            pixels: context.getImageData(0, 0, decoded.width, decoded.height),
          };
          source.current = base;
        }
        if (!live) {
          return;
        }
        const view = useTraceViewStore.getState();
        if (isNeutral(state)) {
          view.setOverride(null);
          return;
        }
        const { width, height } = base.pixels;
        const copy = new Uint8ClampedArray(base.pixels.data);
        editPixels(copy, width, height, state);
        const image = await createImageBitmap(new ImageData(copy, width, height));
        if (live) {
          view.setOverride({ imageId: base.imageId, image });
        }
      })().catch(() => undefined);
    }, 60);
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [state, bitmapId]);
  useEffect(() => () => useTraceViewStore.getState().setOverride(null), []);

  const set = <K extends keyof PictureState>(key: K, value: PictureState[K]) =>
    setState((current) => ({ ...current, [key]: value }));

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (request === null || busy) {
      return;
    }
    setBusy(true);
    void editPicture(request)
      .then(() => {
        source.current = null;
        useTraceViewStore.getState().setOverride(null);
        setState(initialPictureState());
      })
      .finally(() => setBusy(false));
  };

  return (
    <form className="edit-picture-form" onSubmit={onSubmit}>
      <p className="edit-picture-form__hint">{text.hint}</p>
      {bitmapId === null ? (
        <p className="edit-picture-form__notice" role="status">
          {text.needsBitmap}
        </p>
      ) : null}
      <fieldset className="edit-picture-form__group">
        <legend className="edit-picture-form__legend">{text.adjust}</legend>
        <SliderField
          label={text.contrast}
          max={100}
          min={-100}
          onChange={(v) => set('contrast', v)}
          value={state.contrast}
        />
        <SliderField
          label={text.brightness}
          max={100}
          min={-100}
          onChange={(v) => set('brightness', v)}
          value={state.brightness}
        />
        <SliderField
          label={text.gamma}
          max={5}
          min={0.2}
          onChange={(v) => set('gamma', v)}
          step={0.1}
          value={state.gamma}
        />
        <CheckField checked={state.invert} label={text.invert} onChange={(v) => set('invert', v)} />
        <CheckField
          checked={state.greyscale}
          label={text.greyscale}
          onChange={(v) => set('greyscale', v)}
        />
      </fieldset>
      <fieldset className="edit-picture-form__group">
        <legend className="edit-picture-form__legend">{text.border}</legend>
        <ChoiceField
          label={text.borderType}
          onChange={(v) => set('border', v)}
          options={BORDERS}
          value={state.border}
        />
        <SliderField
          disabled={state.border === 'none'}
          label={text.fade}
          max={50}
          min={0}
          onChange={(v) => set('fade', v)}
          unit="%"
          value={state.fade}
        />
      </fieldset>
      {lastError === null ? null : (
        <p className="edit-picture-form__notice" role="alert">
          {lastError}
        </p>
      )}
      <FormButtons
        onClose={closeForm}
        onPrimary={() => undefined}
        onSecondary={() => setState(initialPictureState())}
        primaryWhy={firstWhy([busy, strings.why.busy], [request === null, strings.why.fixValues])}
        primaryLabel={text.apply}
        secondaryLabel={text.reset}
        submit
      />
    </form>
  );
}
