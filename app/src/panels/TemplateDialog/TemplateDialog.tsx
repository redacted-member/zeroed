import { useEffect, useMemo, useRef, useState, type FormEvent, type JSX } from 'react';

import { getKernel } from '../../bridge/client';
import { strings } from '../../lib/formStrings';
import { useDocumentStore } from '../../state/documentStore';
import { useTemplateStore } from '../../state/templateStore';
import {
  draftProblems,
  groupTemplates,
  initialDraft,
  saveArgs,
  templateDetails,
  templateExists,
  templateSize,
  userCategories,
  type SaveDraft,
} from './model';

import './TemplateDialog.scss';
import { dialogKeyDown } from '../../lib/dialogKeys';
import { ButtonFace } from '../../forms/ButtonFace/ButtonFace';
import { whyTip } from '../../shell/Tooltip/model';

const text = strings.templates;

/** File > New from Template and File > Save as Template, modal over the window. */
export function TemplateDialog(): JSX.Element {
  const kind = useTemplateStore((state) => state.dialog);
  const close = useTemplateStore((state) => state.close);
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialog.current;
    if (element === null) {
      return;
    }
    if (kind !== null && !element.open) {
      element.showModal();
    } else if (kind === null && element.open) {
      element.close();
    }
  }, [kind]);

  return (
    <dialog
      aria-labelledby="template-dialog-title"
      className="template-dialog"
      onClose={close}
      onKeyDown={dialogKeyDown(close)}
      ref={dialog}
    >
      {kind === 'new' ? <NewFromTemplate onClose={close} /> : null}
      {kind === 'save' ? <SaveAsTemplate onClose={close} /> : null}
    </dialog>
  );
}

function NewFromTemplate({ onClose }: { onClose: () => void }): JSX.Element {
  const list = useTemplateStore((state) => state.list);
  const loading = useTemplateStore((state) => state.loading);
  const error = useTemplateStore((state) => state.error);
  const newFrom = useTemplateStore((state) => state.newFrom);
  const [picked, setPicked] = useState<string | null>(null);
  const [pickError, setPickError] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyWhy = busy ? strings.why.busy : undefined;
  const groups = useMemo(() => groupTemplates(list?.templates ?? []), [list]);

  useEffect(() => {
    void useTemplateStore.getState().load();
  }, []);

  const onSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (picked === null) {
      setPickError(true);
      return;
    }
    setBusy(true);
    void newFrom(picked).then((ok) => {
      setBusy(false);
      if (ok) {
        onClose();
      }
    });
  };

  return (
    <form className="template-dialog__body" noValidate onSubmit={onSubmit}>
      <header className="template-dialog__header">
        <h2 className="template-dialog__title" id="template-dialog-title">
          {text.newTitle}
        </h2>
      </header>
      <div className="template-dialog__content">
        <p className="template-dialog__hint">{text.newHint}</p>
        {list === null && loading ? <p className="template-dialog__hint">{text.loading}</p> : null}
        {list !== null && groups.length === 0 ? (
          <p className="template-dialog__hint">{text.empty}</p>
        ) : null}
        {groups.map((group) => (
          <fieldset className="template-dialog__group" key={group.key}>
            <legend className="template-dialog__legend">{group.title}</legend>
            <ul className="template-dialog__list">
              {group.items.map((template) => (
                <li className="template-dialog__entry" key={template.path}>
                  <label className="template-dialog__item">
                    <input
                      checked={picked === template.path}
                      className="template-dialog__radio"
                      name="template-dialog-pick"
                      onChange={() => {
                        setPicked(template.path);
                        setPickError(false);
                      }}
                      type="radio"
                      value={template.path}
                    />
                    <span className="template-dialog__name">{template.name}</span>
                    <span className="template-dialog__size">{templateSize(template)}</span>
                    <span className="template-dialog__details">{templateDetails(template)}</span>
                  </label>
                </li>
              ))}
            </ul>
          </fieldset>
        ))}
        {list !== null && list.warnings.length > 0 ? (
          <p className="template-dialog__hint">{text.warnings(list.warnings.length)}</p>
        ) : null}
      </div>
      {pickError ? (
        <p className="template-dialog__error" role="alert">
          {text.errorPick}
        </p>
      ) : null}
      {error === null ? null : (
        <p className="template-dialog__error" role="alert">
          {error}
        </p>
      )}
      <footer className="template-dialog__footer">
        <p className="template-dialog__path">{list === null ? '' : text.folder(list.userDir)}</p>
        <button
          className="template-dialog__button"
          onClick={onClose}
          type="button"
          data-tip={strings.form.cancel}
        >
          <ButtonFace icon="dismiss" label={strings.form.cancel} />
        </button>
        <button
          className="template-dialog__button template-dialog__button--primary"
          disabled={busyWhy !== undefined}
          type="submit"
          data-tip={whyTip(text.open, busyWhy)}
        >
          <ButtonFace icon="fileOpen" label={text.open} />
        </button>
      </footer>
    </form>
  );
}

function SaveAsTemplate({ onClose }: { onClose: () => void }): JSX.Element {
  const path = useDocumentStore((state) => state.path);
  const list = useTemplateStore((state) => state.list);
  const error = useTemplateStore((state) => state.error);
  const save = useTemplateStore((state) => state.save);
  const [draft, setDraft] = useState<SaveDraft>(() => initialDraft(path));
  const [showProblems, setShowProblems] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyWhy = busy ? strings.why.busy : undefined;
  const problems = draftProblems(draft);
  const categories = useMemo(() => userCategories(list?.templates ?? []), [list]);

  useEffect(() => {
    void useTemplateStore.getState().load();
  }, []);

  const edit = (patch: Partial<SaveDraft>) => setDraft((current) => ({ ...current, ...patch }));

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setShowProblems(true);
    let overwrite = false;
    if (templateExists(list?.templates ?? [], draft.name, draft.category)) {
      overwrite = await getKernel().confirm(text.overwrite(draft.name.trim()));
      if (!overwrite) {
        return;
      }
    }
    const args = saveArgs(draft, overwrite);
    if (args === null) {
      return;
    }
    setBusy(true);
    const ok = await save(args);
    setBusy(false);
    if (ok) {
      onClose();
    }
  };

  const nameProblem = showProblems ? problems.name : undefined;
  const categoryProblem = showProblems ? problems.category : undefined;

  return (
    <form className="template-dialog__body" noValidate onSubmit={(event) => void onSubmit(event)}>
      <header className="template-dialog__header">
        <h2 className="template-dialog__title" id="template-dialog-title">
          {text.saveTitle}
        </h2>
      </header>
      <div className="template-dialog__content">
        <p className="template-dialog__hint">{text.saveHint}</p>
        <div className="template-dialog__row">
          <label className="template-dialog__label" htmlFor="template-dialog-name">
            {text.name}
          </label>
          <input
            aria-describedby={nameProblem === undefined ? undefined : 'template-dialog-name-error'}
            aria-invalid={nameProblem !== undefined}
            className="template-dialog__field"
            id="template-dialog-name"
            onChange={(event) => edit({ name: event.target.value })}
            type="text"
            value={draft.name}
          />
          {nameProblem === undefined ? null : (
            <p className="template-dialog__field-error" id="template-dialog-name-error">
              {nameProblem}
            </p>
          )}
        </div>
        <div className="template-dialog__row">
          <label className="template-dialog__label" htmlFor="template-dialog-category">
            {text.category}
          </label>
          <input
            aria-describedby="template-dialog-category-hint"
            aria-invalid={categoryProblem !== undefined}
            className="template-dialog__field"
            id="template-dialog-category"
            list="template-dialog-categories"
            onChange={(event) => edit({ category: event.target.value })}
            type="text"
            value={draft.category}
          />
          <datalist id="template-dialog-categories">
            {categories.map((category) => (
              <option key={category} value={category} />
            ))}
          </datalist>
          <p className="template-dialog__field-hint" id="template-dialog-category-hint">
            {categoryProblem ?? text.categoryHint}
          </p>
        </div>
        <label className="template-dialog__check">
          <input
            checked={draft.design}
            onChange={(event) => edit({ design: event.target.checked })}
            type="checkbox"
          />
          {text.design}
        </label>
        <label className="template-dialog__check">
          <input
            checked={draft.design && draft.toolpaths}
            disabled={!draft.design}
            onChange={(event) => edit({ toolpaths: event.target.checked })}
            type="checkbox"
          />
          {text.toolpaths}
        </label>
      </div>
      {error === null ? null : (
        <p className="template-dialog__error" role="alert">
          {error}
        </p>
      )}
      <footer className="template-dialog__footer">
        <p className="template-dialog__path">{list === null ? '' : text.folder(list.userDir)}</p>
        <button
          className="template-dialog__button"
          onClick={onClose}
          type="button"
          data-tip={strings.form.cancel}
        >
          <ButtonFace icon="dismiss" label={strings.form.cancel} />
        </button>
        <button
          className="template-dialog__button template-dialog__button--primary"
          disabled={busyWhy !== undefined}
          type="submit"
          data-tip={whyTip(text.save, busyWhy)}
        >
          <ButtonFace icon="fileSave" label={text.save} />
        </button>
      </footer>
    </form>
  );
}
