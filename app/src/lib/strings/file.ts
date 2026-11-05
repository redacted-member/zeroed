// File handling, job sheet and printing. The job setup, templates, PDF
// page and document variables strings, used only by forms and dialogs
// that load on demand, are in strings/jobForms.ts.

export const fileStrings = {
  file: {
    openTitle: 'Open Job',
    saveTitle: 'Save Job As',
    importTitle: 'Import Vectors',
    exportTitle: 'Export Vectors',
    jobFiles: 'Zeroed Jobs',
    vectorFiles: 'Vector Files',
    svgFiles: 'SVG',
    dxfFiles: 'DXF',
    epsFiles: 'EPS',
    aiFiles: 'Illustrator',
    pdfFiles: 'PDF',
    templateFiles: 'Toolpath Templates',
    saveTemplateTitle: 'Save Toolpath Template',
    loadTemplateTitle: 'Load Toolpath Template',
    discardChanges: 'This job has changes that have not been saved. Discard them?',
    untitled: 'Untitled',
    allSupported: 'All Supported Files',
    bitmapFiles: 'Bitmap Images',
    meshFiles: '3D Models',
    clipFiles: 'Clipart',
    unsupportedFile: (name: string) => `Zeroed cannot open ${name}.`,
  },
  recovery: {
    showReport:
      'Zeroed quit unexpectedly last time and wrote a crash report on this Mac. Show the report?',
    backupFound: (job: string, backup: string, kept: string) =>
      `Zeroed quit unexpectedly while ${job} had changes that had not been saved. A backup of them was kept at ${backup}.\n\nRestore opens it now; Open Later keeps it in the same folder as ${kept} for File > Open; Discard deletes it.`,
    restore: 'Restore',
    openLater: 'Open Later',
    discard: 'Discard',
  },
  jobSheet: {
    dialogTitle: 'Create Job Sheet',
    htmlFiles: 'HTML Page',
    suffix: 'Job Sheet',
    noToolpaths: 'Calculate a toolpath before creating a job sheet.',
  },
  printing: {
    previewTitle: 'Print Preview',
    print: 'Print...',
    close: 'Close',
    imageAlt: 'The 2D view as it prints',
    noView: 'Show the 2D view to print it.',
  },
} as const;
