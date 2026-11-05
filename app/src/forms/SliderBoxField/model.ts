// Pure rules of a slider paired with a number box.

/** The slider's position for a value, inside its range. */
export function sliderPosition(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.min(max, Math.max(min, value));
}

/** The value typed in the box, or null while it is not a number in the range. */
export function typedValue(text: string, min: number, max: number): number | null {
  if (text.trim() === '') {
    return null;
  }
  const value = Number(text);
  if (!Number.isFinite(value) || value < min || value > max) {
    return null;
  }
  return value;
}

/** The slider's position for a value when the slider runs from `max` at
 * its start to `min` at its end (Coarse to Fine), or the value for such a
 * position: the mapping is its own inverse. */
export function reversedPosition(value: number, min: number, max: number): number {
  // Rounded so that stepping the slider gives clean values such as 0.3.
  return Math.round((min + max - sliderPosition(value, min, max)) * 1e9) / 1e9;
}
