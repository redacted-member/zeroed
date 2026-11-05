// Form state that follows the job units: the fields are text in the display
// unit, convert when the unit changes, and live copies are kept in refs for
// pointer handlers that run outside of render.

import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';

import type { Units } from '../bridge/types';
import { useDocumentStore } from '../state/documentStore';

export interface FormStateHandle<T> {
  state: T;
  setState: Dispatch<SetStateAction<T>>;
  stateRef: MutableRefObject<T>;
  units: Units;
  unitsRef: MutableRefObject<Units>;
}

export function useFormState<T>(
  initial: (units: Units) => T,
  convert: (state: T, from: Units, to: Units) => T,
): FormStateHandle<T> {
  const units = useDocumentStore((store) => store.job.units);
  const [state, setState] = useState<T>(() => initial(units));
  const stateRef = useRef(state);
  const unitsRef = useRef(units);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    const previous = unitsRef.current;
    if (previous !== units) {
      setState((current) => convert(current, previous, units));
      unitsRef.current = units;
    }
  }, [units, convert]);

  return { state, setState, stateRef, units, unitsRef };
}
