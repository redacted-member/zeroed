import { useCallback, useEffect, useId, useState, type JSX } from 'react';

import { getKernel } from '../../bridge/client';
import { api } from '../../bridge/formApi';
import type { FurnitureJoint, JoineryStyle, StockThickness } from '../../bridge/furniture';
import type { JointParams } from '../../bridge/joints';
import { furnitureStrings } from '../../lib/strings/furniture';
import { useAssemblyStore } from '../../state/assemblyStore';
import { useDocumentStore } from '../../state/documentStore';
import { useFurnitureStore, type Session } from '../../state/furnitureStore';
import { firstWhy, whyTip } from '../../shell/Tooltip/model';
import { useUiStore } from '../../state/uiStore';
import { ButtonFace } from '../ButtonFace/ButtonFace';
import { FurnitureJoinery } from '../FurnitureJoinery/FurnitureJoinery';
import { FurnitureLists } from '../FurnitureLists/FurnitureLists';
import { FurnitureParams } from '../FurnitureParams/FurnitureParams';
import { fieldsOf, followValues, templateId, valuesOf } from './model';

import './FurnitureDesigner.scss';

const text = furnitureStrings.furniture;

/**
 * The Furniture Designer, docked over the Assembly View: a template's
 * parameters, the joinery, the lists, and Create or Update. Every change
 * previews live in the Assembly View; a made design is made again when the
 * change settles.
 */
export function FurnitureDesigner(): JSX.Element {
  const session = useFurnitureStore((s) => s.session);

  // The designer shows over the Assembly View: its pieces stay in the 3D
  // view while the form is open.
  useEffect(() => {
    const ui = useUiStore.getState();
    if (ui.viewMode === '2d') {
      ui.setViewMode('3d');
    }
    void useAssemblyStore.getState().open();
    return () => {
      useAssemblyStore.getState().close();
      // The design stays open across a remount (development double
      // mounts); it closes once the form is really gone.
      setTimeout(() => {
        if (useUiStore.getState().activeForm !== 'furnitureDesigner') {
          useFurnitureStore.getState().endSession();
        }
      }, 0);
    };
  }, []);

  if (session === null) {
    return <NoSession />;
  }
  return (
    <DesignerBody key={`${session.template.id}:${session.design ?? 'new'}`} session={session} />
  );
}

/** Opened with no design: back to the Assembly View, which offers New Furniture. */
function NoSession(): JSX.Element {
  return (
    <div className="furniture-designer">
      <p className="furniture-designer__hint">{text.noDesigns}</p>
      <div className="furniture-designer__buttons">
        <button
          className="furniture-designer__button"
          onClick={() => useUiStore.getState().openForm('assemblyView')}
          type="button"
        >
          <ButtonFace icon="dismiss" label={text.designer.close} />
        </button>
      </div>
    </div>
  );
}

function DesignerBody({ session }: { session: Session }): JSX.Element {
  const units = useDocumentStore((s) => s.job.units);
  const revision = useDocumentStore((s) => s.revision);
  const store = useFurnitureStore;
  const connections = useFurnitureStore((s) => s.connections);
  const busy = useFurnitureStore((s) => s.busy);
  const error = useFurnitureStore((s) => s.error);
  const notice = useFurnitureStore((s) => s.notice);
  const summary = useFurnitureStore((s) => s.summary);
  const preview = useFurnitureStore((s) => s.preview);
  const params = session.template.params;
  const [stocks, setStocks] = useState<StockThickness[]>([]);
  useEffect(() => {
    let live = true;
    // Stock thicknesses are a convenience: without them the boxes still take any size.
    api.furniture
      .materials()
      .then((m) => {
        if (live) {
          setStocks(m.thicknesses);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);
  const [fields, setFields] = useState<Record<string, string>>(() =>
    fieldsOf(params, session.values, units),
  );
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState<string | null>(null);
  const nameId = useId();
  const templateNameId = useId();
  const made = session.design !== null;

  // Fields follow the job unit, and values changed elsewhere (a size arrow
  // dragged in the view); a field whose text already gives its value keeps
  // the text, so a typed calculation stays.
  const [shownUnits, setShownUnits] = useState(units);
  const [shownValues, setShownValues] = useState(session.values);
  if (shownUnits !== units) {
    setShownUnits(units);
    setShownValues(session.values);
    setFields(fieldsOf(params, session.values, units));
  } else if (shownValues !== session.values) {
    setShownValues(session.values);
    setFields(followValues(params, fields, shownValues, session.values, units));
  }

  const onChange = useCallback(
    (key: string, typed: string, release: boolean) => {
      const next = { ...fields, [key]: typed };
      setFields(next);
      const last = store.getState().session?.values ?? {};
      const result = valuesOf(params, next, last, units);
      setErrors(result.errors);
      if (result.errors[key] === undefined) {
        store.getState().setValues(result.values, release);
      }
    },
    [fields, params, store, units],
  );

  const onRelease = useCallback(() => store.getState().release(), [store]);
  const onStyle = useCallback(
    (style: JoineryStyle) => void store.getState().setStyle(style),
    [store],
  );
  const onClassJoint = useCallback(
    (cls: string, joint: FurnitureJoint | null, sizes?: JointParams) =>
      void store.getState().setClassJoint(cls, joint, sizes),
    [store],
  );
  const onSettings = useCallback(() => {
    const now = store.getState().session;
    if (now !== null) {
      store.getState().setValues(now.values, true);
    }
  }, [store]);
  const onNotice = useCallback((n: string) => store.getState().setNotice(n), [store]);
  const onError = useCallback((e: string) => store.getState().setError(e), [store]);

  const create = async () => {
    const ok = await store.getState().create();
    if (ok) {
      const s = store.getState().summary;
      store.getState().setNotice(
        s === null
          ? null
          : text.designer.made(
              s.panels,
              s.sheets.reduce((sum, sheet) => sum + sheet.count, 0),
            ),
      );
    }
  };

  const update = () => {
    const now = store.getState().session;
    if (now !== null) {
      store.getState().setValues(now.values, true);
    }
  };

  const remove = async () => {
    if (await getKernel().confirm(text.designer.confirmRemove(session.name))) {
      if (await store.getState().remove(false)) {
        useUiStore.getState().openForm('assemblyView');
      }
    }
  };

  const saveTemplate = async () => {
    if (saving === null || saving.trim() === '') {
      return;
    }
    const name = saving.trim();
    if (await store.getState().saveTemplate(templateId(name), name)) {
      store.getState().setNotice(text.designer.savedTemplate(name));
      setSaving(null);
    }
  };

  const issues = [...(preview?.issues ?? []), ...(summary?.issues ?? [])];
  const why = firstWhy([Object.keys(errors).length > 0, text.designer.fixFields]);

  return (
    <form
      className="furniture-designer"
      onSubmit={(event) => {
        event.preventDefault();
        if (made) {
          update();
        } else {
          void create();
        }
      }}
    >
      <p className="furniture-designer__hint">{text.designer.hint}</p>
      <div className="furniture-designer__name">
        <label className="furniture-designer__label" htmlFor={nameId}>
          {text.designer.name}
        </label>
        <input
          className="furniture-designer__input"
          id={nameId}
          onBlur={() => {
            if (made) {
              update();
            }
          }}
          onChange={(event) => store.getState().setName(event.target.value)}
          type="text"
          value={session.name}
        />
        <p className="furniture-designer__hint">{text.designer.template(session.template.name)}</p>
      </div>
      <FurnitureParams
        errors={errors}
        fields={fields}
        onChange={onChange}
        onRelease={onRelease}
        params={params}
        stocks={stocks}
        units={units}
      />
      <FurnitureJoinery
        classes={session.template.jointClasses}
        connections={connections}
        disabled={false}
        joints={session.joints}
        onClassJoint={onClassJoint}
        onSettings={onSettings}
        onStyle={onStyle}
        units={units}
      />
      <FurnitureLists
        design={session.design}
        name={session.name}
        onError={onError}
        onNotice={onNotice}
        revision={revision}
        units={units}
      />
      {issues.length === 0 ? null : (
        <section className="furniture-designer__issues" aria-label={text.designer.issues}>
          <h3 className="furniture-designer__heading">{text.designer.issues}</h3>
          <ul className="furniture-designer__list">
            {issues.map((issue, i) => (
              <li key={`${issue.panel ?? ''}:${issue.connection ?? ''}:${i}`}>{issue.message}</li>
            ))}
          </ul>
        </section>
      )}
      <p aria-live="polite" className="furniture-designer__status">
        {busy ? text.designer.previewing : (notice ?? '')}
      </p>
      {error === null ? null : (
        <p className="furniture-designer__error" role="alert">
          {error}
        </p>
      )}
      {saving === null ? null : (
        <div className="furniture-designer__save">
          <label className="furniture-designer__label" htmlFor={templateNameId}>
            {text.designer.templateName}
          </label>
          <input
            className="furniture-designer__input"
            id={templateNameId}
            onChange={(event) => setSaving(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                void saveTemplate();
              } else if (event.key === 'Escape') {
                event.preventDefault();
                setSaving(null);
              }
            }}
            type="text"
            value={saving}
          />
          <div className="furniture-designer__buttons">
            <button
              className="furniture-designer__button"
              onClick={() => void saveTemplate()}
              type="button"
            >
              <ButtonFace icon="fileSave" label={text.designer.saveTemplateOk} />
            </button>
            <button
              className="furniture-designer__button"
              onClick={() => setSaving(null)}
              type="button"
            >
              <ButtonFace icon="dismiss" label={text.designer.cancel} />
            </button>
          </div>
        </div>
      )}
      <div className="furniture-designer__buttons">
        <button
          className="furniture-designer__button furniture-designer__button--primary"
          data-tip={whyTip(made ? text.designer.updateTip : text.designer.createTip, why)}
          disabled={why !== undefined}
          type="submit"
        >
          <ButtonFace
            icon={made ? 'formApply' : 'jointCreate'}
            label={made ? text.designer.update : text.designer.create}
          />
        </button>
        {made ? (
          <>
            <button
              className="furniture-designer__button"
              data-tip={text.designer.saveTemplateTip}
              onClick={() => setSaving(session.name)}
              type="button"
            >
              <ButtonFace icon="fileNewTemplate" label={text.designer.saveTemplate} />
            </button>
            <button
              className="furniture-designer__button"
              data-tip={text.designer.removeTip}
              onClick={() => void remove()}
              type="button"
            >
              <ButtonFace icon="editDelete" label={text.designer.remove} />
            </button>
          </>
        ) : null}
        <button
          className="furniture-designer__button"
          onClick={() => useUiStore.getState().openForm('assemblyView')}
          type="button"
        >
          <ButtonFace icon="dismiss" label={text.designer.close} />
        </button>
      </div>
    </form>
  );
}
