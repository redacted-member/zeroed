// Spell checking for text the user types into a design (Create Text, text
// on a curve or arc, dimension notes, project notes). The platform does the
// checking: Chromium and the macOS web view both underline unknown words in
// red and list suggestions, Ignore and Learn in the right-click menu, using
// the system dictionaries and languages. The kernel never sees any of it,
// so it stays platform agnostic. Autocorrection stays off so carved text is
// never changed behind the user's back.

/** Props for a text entry that is spell checked but never autocorrected. */
export const SPELL_CHECKED = {
  spellCheck: true,
  autoCorrect: 'off',
  autoCapitalize: 'off',
} as const;
