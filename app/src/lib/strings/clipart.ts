// Clipart tab.

export const clipartStrings = {
  clipart: {
    badScale: 'The scale must be a percentage from 1 to 10000.',
    inserted: (name: string) => `Inserted ${name}.`,
    saved: (path: string) => `Saved ${path}`,
    saveTitle: 'Save Component as Clipart',
    files: 'Clipart',
    untitled: 'Clipart',
  },
} as const;
