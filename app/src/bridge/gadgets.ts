// JSON shapes of the gadgets.* commands (Lua add-ons run by the kernel).

export type GadgetFieldKind = 'number' | 'length' | 'choice' | 'check' | 'text' | 'tool';

export type GadgetOrigin = 'bundled' | 'user';

/** A value the dialog hands to the script. Lengths are millimetres. */
export type GadgetValue = number | boolean | string;

export interface GadgetOption {
  value: string;
  label: string;
}

/** One dialog field as the kernel describes it. */
export interface GadgetField {
  id: string;
  label: string;
  kind: GadgetFieldKind;
  /** A number (millimetres for lengths, a tool database id for tools, 0 for
   * none), a bool for checks, text otherwise. */
  default: GadgetValue;
  options?: GadgetOption[];
  /** Smallest legal value, millimetres for lengths. */
  min?: number;
  max?: number;
  tooltip?: string;
  /** Heading of the section the field sits in; neighbours share one. */
  group?: string;
  /** Id of a check field that must be ticked for this field to be used. */
  enabledBy?: string;
}

export interface GadgetSummary {
  name: string;
  title: string;
  description: string;
  origin: GadgetOrigin;
  /** Set when the script does not load. */
  error?: string;
}

export interface GadgetDescription {
  name: string;
  title: string;
  description: string;
  origin: GadgetOrigin;
  fields: GadgetField[];
  /** Set for a gadget that works on the selection: what to tell the user
   * while nothing is selected. */
  needsSelection?: string;
  /** The values of the gadget's last run (lengths in millimetres), when it ran before. */
  lastValues?: Record<string, GadgetValue>;
}

export interface GadgetRunReply {
  messages: string[];
  revision: number;
  /** The toolpaths the run made, or the edited toolpath first; absent from older kernels. */
  toolpathIds?: number[];
}
