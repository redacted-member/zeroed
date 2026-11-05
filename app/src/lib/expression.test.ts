import { describe, expect, it } from 'vitest';

import { evaluate, isVariableName } from './expression';

describe('evaluate', () => {
  it('reads plain numbers as before', () => {
    expect(evaluate('12.5')).toBe(12.5);
    expect(evaluate(' -3 ')).toBe(-3);
    expect(evaluate('.5')).toBe(0.5);
    expect(evaluate('1e3')).toBe(1000);
    expect(evaluate('')).toBeNull();
    expect(evaluate('abc')).toBeNull();
    expect(evaluate('12mm')).toBeNull();
  });

  it('does arithmetic with the usual precedence and brackets', () => {
    expect(evaluate('2+3*4')).toBe(14);
    expect(evaluate('(2+3)*4')).toBe(20);
    expect(evaluate('=10/4')).toBe(2.5);
    expect(evaluate('-(1+2)*-2')).toBe(6);
    expect(evaluate('1/0')).toBeNull();
    expect(evaluate('(1+2')).toBeNull();
    expect(evaluate('1+')).toBeNull();
  });

  it('uses document variables in square brackets', () => {
    const vars = { width: 300, gap: 6.35 };
    expect(evaluate('[width]/2', vars)).toBe(150);
    expect(evaluate('{width}/2', vars)).toBe(150);
    expect(evaluate('={width} - 2*{gap}', vars)).toBeCloseTo(287.3);
    expect(evaluate('{width', vars)).toBeNull();
    expect(evaluate('[width] - 2*[gap]', vars)).toBeCloseTo(287.3);
    expect(evaluate('[depth]', vars)).toBeNull();
    expect(evaluate('[toString]', vars)).toBeNull();
    expect(evaluate('[width', vars)).toBeNull();
  });

  it('checks variable names', () => {
    expect(isVariableName('width_2')).toBe(true);
    expect(isVariableName('2width')).toBe(false);
    expect(isVariableName('a b')).toBe(false);
  });
});

describe('calculation letters', () => {
  const job = { width: 600, height: 400, thickness: 18, foot: 304.8 };

  it('reads the material, pi and the unit conversions', () => {
    expect(evaluate('w/2', {}, job)).toBe(300);
    expect(evaluate('X=', {}, job)).toBe(600);
    expect(evaluate('X', {}, job)).toBeNull();
    expect(evaluate('H*2', {}, job)).toBe(800);
    expect(evaluate('y*1', {}, job)).toBe(400);
    expect(evaluate('t-0.25', {}, job)).toBe(17.75);
    expect(evaluate('=Z', {}, job)).toBe(18);
    expect(evaluate('P*10^2', {}, job)).toBeCloseTo(314.159265);
    expect(evaluate('25.4*i', {}, job)).toBeCloseTo(1);
    expect(evaluate('2*M', {}, job)).toBeCloseTo(50.8);
  });

  it('takes powers, feet and a trailing equals sign', () => {
    expect(evaluate('2^3^2', {}, job)).toBe(512);
    expect(evaluate('-2^2', {}, job)).toBe(-4);
    const inches = { ...job, foot: 12 };
    expect(evaluate("2'+10", {}, inches)).toBe(34);
    expect(evaluate('3+(4/5)=', {}, job)).toBeCloseTo(3.8);
  });

  it('still refuses other words and letters stuck to numbers', () => {
    expect(evaluate('12mm', {}, job)).toBeNull();
    expect(evaluate('q', {}, job)).toBeNull();
    expect(evaluate('width', {}, job)).toBeNull();
  });
});
