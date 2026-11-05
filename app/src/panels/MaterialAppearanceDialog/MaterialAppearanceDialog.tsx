import { useId, useState, type FormEvent, type JSX } from 'react';

import { getKernel } from '../../bridge/client';
import type { Material3d } from '../../bridge/prefs';
import {
  addTexture,
  addTextureCategory,
  addTextureValue,
  categoryName,
  textureOf,
  texturesByCategory,
  textureValue,
} from '../../forms/PreviewToolpathsForm/model';
import { strings } from '../../lib/formStrings';
import { strings as formStrings } from '../../lib/formStrings';
import { prefsStrings } from '../../lib/strings/prefs';
import { usePrefsStore } from '../../state/prefsStore';
import { MATERIALS } from '../../views/View3D/appearance';
import { ModalDialog } from '../ModalDialog/ModalDialog';
import {
  chooseColour,
  chooseMaterial,
  chooseTexture,
  materialChoices,
  usesSolidColour,
  type MaterialDraft,
} from './model';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';

import './MaterialAppearanceDialog.scss';

const text = strings.jobSetup;
const textures = formStrings.toolpaths.preview;
const CHOICES = materialChoices(MATERIALS);

/** List entries that add a texture picture. */
const ADD_TEXTURE = 'add-texture';
const CREATE_CATEGORY = 'create-category';

const TEXTURE_FILTERS = [
  {
    name: strings.file.bitmapFiles,
    extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp'],
  },
];

function choiceLabel(material: Material3d): string {
  return material === 'solid' ? text.useSolidColour : prefsStrings.preferences.materials[material];
}

interface MaterialAppearanceDialogProps {
  material: Material3d;
  colour: string;
  /** The texture picture shown, 0 for none. */
  texture?: number;
  /** OK: the chosen material, solid color and texture. */
  onApply: (draft: MaterialDraft) => void;
  onClose: () => void;
}

/**
 * Material Appearance: the material list (Use Solid Color, the materials
 * and the texture pictures by category, with entries to add pictures), and
 * a color picker for the solid color.
 */
export function MaterialAppearanceDialog(props: MaterialAppearanceDialogProps): JSX.Element {
  const { material, colour, texture = 0, onApply, onClose } = props;
  const [draft, setDraft] = useState<MaterialDraft>({ material, colour, texture });
  const library = usePrefsStore((store) => store.prefs.view3d.textures ?? []);
  const id = useId();
  const shownTexture = draft.texture ?? 0;
  const textured = shownTexture !== 0 && library.some((t) => t.id === shownTexture);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    onApply(textured ? draft : { ...draft, texture: 0 });
    onClose();
  };

  const pick = async (value: string) => {
    // A new category is named, then gets its first picture.
    const category =
      value === ADD_TEXTURE
        ? textures.myTextures
        : value === CREATE_CATEGORY
          ? categoryName(window.prompt(textures.categoryPrompt))
          : addTextureCategory(value);
    if (value === ADD_TEXTURE || value === CREATE_CATEGORY || category !== null) {
      if (category === null) {
        return;
      }
      const path = await getKernel().chooseOpenPath(textures.addTextureTitle, TEXTURE_FILTERS);
      if (path === null) {
        return;
      }
      const added = addTexture(library, path, category);
      if (await usePrefsStore.getState().update({ view3d: { textures: added.textures } })) {
        setDraft((current) => chooseTexture(current, added.id));
      }
      return;
    }
    const picked = textureOf(value);
    setDraft((current) =>
      picked === null
        ? chooseMaterial(current, value as Material3d)
        : chooseTexture(current, picked),
    );
  };

  const removeTexture = async () => {
    const kept = library.filter((t) => t.id !== shownTexture);
    if (await usePrefsStore.getState().update({ view3d: { textures: kept } })) {
      setDraft((current) => chooseTexture(current, 0));
    }
  };

  return (
    <ModalDialog
      className="material-appearance"
      closeLabel={strings.form.cancel}
      footer={
        <button
          className="modal-dialog__button"
          form={`${id}-form`}
          type="submit"
          data-tip={strings.form.ok}
        >
          <ButtonFace icon="formApply" label={strings.form.ok} />
        </button>
      }
      onClose={onClose}
      open
      title={text.materialDialogTitle}
      titleId={`${id}-title`}
    >
      <form className="material-appearance__form" id={`${id}-form`} onSubmit={onSubmit}>
        <label className="material-appearance__label" htmlFor={`${id}-material`}>
          {text.material}
        </label>
        <select
          className="material-appearance__select"
          id={`${id}-material`}
          onChange={(event) => void pick(event.target.value)}
          value={textured ? textureValue(shownTexture) : draft.material}
        >
          {CHOICES.map((value) => (
            <option key={value} value={value}>
              {choiceLabel(value)}
            </option>
          ))}
          {texturesByCategory(library).map((group) => (
            <optgroup key={group.category} label={group.category}>
              {group.textures.map((t) => (
                <option key={t.id} value={textureValue(t.id)}>
                  {t.name}
                </option>
              ))}
              <option value={addTextureValue(group.category)}>{textures.addTexture}</option>
            </optgroup>
          ))}
          {library.length === 0 ? <option value={ADD_TEXTURE}>{textures.addTexture}</option> : null}
          <option value={CREATE_CATEGORY}>{textures.createCategory}</option>
        </select>
        <label className="material-appearance__label" htmlFor={`${id}-colour`}>
          {text.solidColour}
        </label>
        <input
          className="material-appearance__colour"
          disabled={!usesSolidColour(draft) || textured}
          id={`${id}-colour`}
          onChange={(event) => setDraft((current) => chooseColour(current, event.target.value))}
          type="color"
          value={draft.colour}
        />
        {textured ? (
          <button
            className="material-appearance__remove"
            onClick={() => void removeTexture()}
            type="button"
            data-tip={textures.removeTexture}
          >
            <ButtonFace icon="itemRemove" label={textures.removeTexture} />
          </button>
        ) : null}
      </form>
    </ModalDialog>
  );
}
