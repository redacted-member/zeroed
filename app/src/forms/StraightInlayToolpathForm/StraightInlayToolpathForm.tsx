import { useEffect, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import { strings } from '../../lib/formStrings';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { InlayPocketForm } from '../InlayPocketForm/InlayPocketForm';
import { InlayProfileForm } from '../InlayProfileForm/InlayProfileForm';
import { INLAY_GROUPS, KIND_LABELS, kindOf, type InlayKind } from './model';
import { ButtonFace } from '../ButtonFace/ButtonFace';

import './StraightInlayToolpathForm.scss';
import { HelperPicture } from '../HelperPicture/HelperPicture';
import { inlayKindPicture } from '../HelperPicture/pictures/toolpaths';
import { HelperRow } from '../HelperRow/HelperRow';

const text = strings.toolpaths.straightInlay;

/**
 * Create Inlay Toolpath: pick Male Straight or Stepped, Female Pocket or
 * Hole, and that kind's form opens in its place. Editing an inlay toolpath
 * opens the form of the kind it was made as.
 */
export function StraightInlayToolpathForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const editing = useUiStore((store) => store.editingToolpath);
  const [kind, setKind] = useState<InlayKind | null>(null);

  useEffect(() => {
    if (editing === null) {
      return;
    }
    let live = true;
    void api.cam.get(editing).then((detail) => {
      const request = detail.request;
      if (live && request !== null && request.strategy === 'straightInlay') {
        setKind(kindOf(request.params));
      }
    });
    return () => {
      live = false;
    };
  }, [editing]);

  if (kind === 'femalePocket') {
    return <InlayPocketForm />;
  }
  if (kind !== null) {
    return <InlayProfileForm key={kind} kind={kind} />;
  }
  if (editing !== null) {
    return <p className="straight-inlay__hint">{text.loading}</p>;
  }
  return (
    <div className="straight-inlay">
      <p className="straight-inlay__hint">{text.chooserHint}</p>
      {INLAY_GROUPS.map((group) => (
        <fieldset className="straight-inlay__group" key={group.label}>
          <legend className="straight-inlay__legend">{group.label}</legend>
          {group.kinds.map((each) => (
            <HelperRow key={each} picture={<HelperPicture picture={inlayKindPicture(each)} />}>
              <button
                className="straight-inlay__choice"
                onClick={() => setKind(each)}
                type="button"
                data-tip={KIND_LABELS[each]}
              >
                <ButtonFace icon="straightInlay" label={KIND_LABELS[each]} />
              </button>
            </HelperRow>
          ))}
        </fieldset>
      ))}
      <FormButtons onClose={closeForm} />
    </div>
  );
}
