// Arithmetic in numeric fields (the calculation edit boxes):
// numbers, + - * / and ^, brackets, document variables written as {name}
// (or [name], as older jobs saved them), the letters W or X (material
// width), H or Y (height), T or Z (thickness), P (pi), I (millimetres to
// inches) and M (inches to millimetres), and ' after a number for feet,
// optionally after a leading "=". Anything else is not a number.

export type Variables = Readonly<Record<string, number>>;

/** What the calculation letters stand for, in the unit of the job. */
export interface Constants {
  width: number;
  height: number;
  thickness: number;
  /** One foot in the unit of the job (12 in an inch job, 304.8 in mm). */
  foot: number;
}

const NO_JOB: Constants = { width: 0, height: 0, thickness: 0, foot: 304.8 };

let source: () => Variables = () => ({});
let constantsSource: () => Constants = () => NO_JOB;

/** Sets where the calculation letters read the material from. */
export function setConstantSource(read: () => Constants): void {
  constantsSource = read;
}

/** The value of a calculation letter, or undefined for any other letter. */
function letterValue(letter: string, constants: Constants): number | undefined {
  switch (letter.toLowerCase()) {
    case 'w':
    case 'x':
      return constants.width;
    case 'h':
    case 'y':
      return constants.height;
    case 't':
    case 'z':
      return constants.thickness;
    case 'p':
      return Math.PI;
    case 'i':
      return 1 / 25.4;
    case 'm':
      return 25.4;
    default:
      return undefined;
  }
}

/** Sets where numeric fields read the document variables from. */
export function setVariableSource(read: () => Variables): void {
  source = read;
}

/** The document variables numeric fields evaluate with. */
export function currentVariables(): Variables {
  return source();
}

type Op = '+' | '-' | '*' | '/' | '^' | '(' | ')' | "'";

type Token =
  { kind: 'number'; value: number } | { kind: 'op'; op: Op } | { kind: 'name'; name: string };

function tokens(text: string, constants: Constants | null): Token[] | null {
  const out: Token[] = [];
  let i = 0;
  while (i < text.length) {
    const c = text.charAt(i);
    if (c === ' ' || c === '\t') {
      i += 1;
    } else if ("+-*/^()'".includes(c)) {
      out.push({ kind: 'op', op: c as Op });
      i += 1;
    } else if (/[a-z]/i.test(c) && !/[a-z0-9_]/i.test(text.charAt(i + 1))) {
      const value = constants === null ? undefined : letterValue(c, constants);
      if (value === undefined) {
        return null;
      }
      out.push({ kind: 'number', value });
      i += 1;
    } else if (c === '{' || c === '[') {
      const end = text.indexOf(c === '{' ? '}' : ']', i);
      if (end < 0) {
        return null;
      }
      out.push({ kind: 'name', name: text.slice(i + 1, end).trim() });
      i = end + 1;
    } else {
      const match = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(text.slice(i));
      if (match === null) {
        return null;
      }
      out.push({ kind: 'number', value: Number(match[0]) });
      i += match[0].length;
    }
  }
  return out;
}

/**
 * The value of an expression, or null when it does not read or names a
 * variable that is not defined. Division by zero is null too.
 */
export function evaluate(
  text: string,
  variables: Variables = {},
  constants: Constants = constantsSource(),
): number | null {
  const body = text.trim().replace(/^=/, '').replace(/=$/, '');
  // A letter alone is not a number: the letters stand for values only in
  // a calculation (with an operator, or ended by =).
  const calculation = /[-+*/^()'=]/.test(text.trim().replace(/^-/, ''));
  const list = tokens(body, calculation ? constants : null);
  if (list === null || list.length === 0) {
    return null;
  }
  let at = 0;
  const peek = (): Token | undefined => list[at];
  const isOp = (op: string): boolean => {
    const t = peek();
    return t !== undefined && t.kind === 'op' && t.op === op;
  };

  // expr := term (('+' | '-') term)*; term := unary (('*' | '/') unary)*;
  // unary := '-' unary | '+' unary | power; power := feet ('^' unary)?;
  // feet := atom "'"?; atom := number | name | '(' expr ')'
  const expr = (): number | null => {
    let left = term();
    while (left !== null && (isOp('+') || isOp('-'))) {
      const minus = isOp('-');
      at += 1;
      const right = term();
      left = right === null ? null : minus ? left - right : left + right;
    }
    return left;
  };
  const term = (): number | null => {
    let left = unary();
    while (left !== null && (isOp('*') || isOp('/'))) {
      const divide = isOp('/');
      at += 1;
      const right = unary();
      if (right === null || (divide && right === 0)) {
        return null;
      }
      left = divide ? left / right : left * right;
    }
    return left;
  };
  const unary = (): number | null => {
    if (isOp('-') || isOp('+')) {
      const minus = isOp('-');
      at += 1;
      const value = unary();
      return value === null ? null : minus ? -value : value;
    }
    return power();
  };
  const power = (): number | null => {
    const base = feet();
    if (base === null || !isOp('^')) {
      return base;
    }
    at += 1;
    const exponent = unary();
    return exponent === null ? null : base ** exponent;
  };
  const feet = (): number | null => {
    const value = atom();
    if (value !== null && isOp("'")) {
      at += 1;
      return value * constants.foot;
    }
    return value;
  };
  const atom = (): number | null => {
    const t = peek();
    if (t === undefined) {
      return null;
    }
    at += 1;
    if (t.kind === 'number') {
      return t.value;
    }
    if (t.kind === 'name') {
      const value = Object.prototype.hasOwnProperty.call(variables, t.name)
        ? variables[t.name]
        : undefined;
      return value === undefined ? null : value;
    }
    if (t.op === '(') {
      const inner = expr();
      if (inner === null || !isOp(')')) {
        return null;
      }
      at += 1;
      return inner;
    }
    return null;
  };

  const value = expr();
  if (value === null || at !== list.length || !Number.isFinite(value)) {
    return null;
  }
  return value;
}

/** Variable names: a letter, then letters, digits or underscores. */
export function isVariableName(name: string): boolean {
  return /^[A-Za-z][A-Za-z0-9_]*$/.test(name);
}
