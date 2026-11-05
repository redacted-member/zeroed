import { Suspense, type JSX, type ReactNode } from 'react';

import { contextTopic } from '../../lib/helpTopics';
import { strings } from '../../lib/strings';
import { useDialogStore } from '../../state/dialogStore';
import { useUiStore, type FormId } from '../../state/uiStore';

import './FormHost.scss';

interface FormHostProps {
  title: string;
  /** The form shown, whose help the help button opens. */
  form: FormId | null;
  children: ReactNode;
}

/** Holds the open form in its dock, in place of the tab or panel content. */
export function FormHost({ title, form, children }: FormHostProps): JSX.Element {
  const closeForm = useUiStore((state) => state.closeForm);

  return (
    <div className="form-host">
      <header className="form-host__header">
        <h2 className="form-host__title">{title}</h2>
        <button
          aria-label={strings.form.help}
          data-tip={strings.form.help}
          className="form-host__button form-host__help"
          onClick={() => useDialogStore.getState().showHelp(contextTopic(form))}
          type="button"
        >
          ?
        </button>
        <button
          aria-label={strings.form.close}
          data-tip={strings.form.close}
          className="form-host__button"
          onClick={() => closeForm()}
          type="button"
        >
          &times;
        </button>
      </header>
      <div className="form-host__body">
        <Suspense
          fallback={
            <p className="form-host__loading" role="status">
              {strings.form.loading}
            </p>
          }
        >
          {children}
        </Suspense>
      </div>
    </div>
  );
}
