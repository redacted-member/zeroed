import type { JSX } from 'react';

import { strings } from '../../lib/formStrings';
import { useModelStore } from '../../state/modelStore';
import { CheckField } from '../CheckField/CheckField';

import './ProjectField.scss';

interface ProjectFieldProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
}

/**
 * The "Project toolpath onto 3D model" option of flat toolpath forms. It is
 * only offered once there is a model to project onto (a toolpath that was
 * projected can still be switched off).
 */
export function ProjectField({ checked, onChange }: ProjectFieldProps): JSX.Element {
  const text = strings.toolpaths.project;
  const hasModel = useModelStore((store) => store.components.length > 0);
  return (
    <div className="project-field">
      <CheckField
        checked={checked}
        disabled={!hasModel && !checked}
        label={text.label}
        onChange={onChange}
      />
      {checked ? <p className="project-field__hint">{text.hint}</p> : null}
    </div>
  );
}
