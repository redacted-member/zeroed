import { useMemo, useRef, useState, type JSX } from 'react';

import { api } from '../../bridge/formApi';
import type { CrossSectionArgs } from '../../bridge/modelTools';
import { strings } from '../../lib/formStrings';
import { runModelAction } from '../../lib/runModelAction';
import { useDrawController } from '../../lib/useDrawController';
import { useDocumentStore } from '../../state/documentStore';
import { useDrawStore, type DrawController } from '../../state/drawStore';
import { useUiStore } from '../../state/uiStore';
import { FormButtons } from '../FormButtons/FormButtons';
import { clickPoint, INITIAL_CROSS_SECTION, sectionPreview, type CrossSectionState } from './model';

import './CrossSectionForm.scss';

const text = strings.model.crossSection;

export function CrossSectionForm(): JSX.Element {
  const closeForm = useUiStore((store) => store.closeForm);
  const [state, setState] = useState<CrossSectionState>(INITIAL_CROSS_SECTION);
  const stateRef = useRef(state);
  const [failure, setFailure] = useState<string | null>(null);

  const controller = useMemo<DrawController>(() => {
    const make = (section: CrossSectionArgs) => {
      void runModelAction(async () => {
        const reply = await api.model.crossSection(section);
        await api.selection.set([reply.id]);
        return reply;
      }).then((reply) => setFailure(reply === null ? useDocumentStore.getState().lastError : null));
    };
    const update = (next: CrossSectionState) => {
      stateRef.current = next;
      setState(next);
    };
    return {
      cursor: 'crosshair',
      pointerDown: (p) => {
        setFailure(null);
        const { state: next, section } = clickPoint(stateRef.current, { x: p.x, y: p.y });
        update(next);
        useDrawStore.getState().setPreview([]);
        if (section !== null) {
          make(section);
        }
      },
      pointerMove: (p) => {
        const start = stateRef.current.start;
        if (start !== null) {
          useDrawStore.getState().setPreview(sectionPreview(start, { x: p.x, y: p.y }));
        }
      },
      pointerUp: () => undefined,
      cancel: () => {
        update(INITIAL_CROSS_SECTION);
        useDrawStore.getState().setPreview([]);
      },
    };
  }, []);
  useDrawController(controller);

  return (
    <div className="cross-section">
      <p className="cross-section__hint">{text.hint}</p>
      <p className="cross-section__step" aria-live="polite">
        {state.start === null ? text.clickStart : text.clickEnd}
      </p>
      {failure === null ? null : (
        <p className="cross-section__notice" role="alert">
          {failure}
        </p>
      )}
      <FormButtons onClose={closeForm} />
    </div>
  );
}
