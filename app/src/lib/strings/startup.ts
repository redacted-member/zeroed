// Strings of lazily loaded areas that the start bundle needs: the form titles
// the registry shows, and the model messages the menus use. The area files
// read these back, so each text is written once.

export const startupStrings = {
  formTitles: {
    createVectors: {
      rectangle: {
        title: 'Draw Rectangle',
      },
      circle: {
        title: 'Draw Circle',
      },
      ellipse: {
        title: 'Draw Ellipse',
      },
      polygon: {
        title: 'Draw Polygon',
      },
      star: {
        title: 'Draw Star',
      },
      polyline: {
        title: 'Draw Line',
      },
      arc: {
        title: 'Draw Arc',
      },
      dimensions: {
        title: 'Dimensions',
      },
      traceBitmap: {
        title: 'Trace Bitmap',
      },
    },
    drawTools: {
      freehand: {
        title: 'Freehand Drawing',
      },
      measure: {
        title: 'Measure - Inspect',
      },
    },
    transform: {
      move: {
        title: 'Move Selection',
      },
      setSize: {
        title: 'Set Size',
      },
      rotate: {
        title: 'Rotate',
      },
      mirror: {
        title: 'Mirror',
      },
      distort: {
        title: 'Distort Object',
      },
      align: {
        title: 'Alignment Tools',
      },
    },
    vectorUnwrapper: {
      title: 'Vector Unwrapper',
    },
    assemblyView: {
      title: 'Assembly View',
    },
    jobSetup: {
      title: 'Job Setup',
    },
    documentVariables: {
      title: 'Document Variables',
    },
    joints: {
      title: 'Joint Creator',
    },
    furnitureDesigner: {
      title: 'Furniture Designer',
    },
    snapSettings: {
      title: 'Snap Settings',
    },
    guidePosition: {
      title: 'Guide Position',
    },
    editObjects: {
      join: {
        title: 'Join Vectors',
      },
      offset: {
        title: 'Offset Vectors',
      },
      fillet: {
        title: 'Create Fillets',
      },
      trim: {
        title: 'Interactive Vector Trim',
      },
      trimObjects: {
        title: 'Trim Objects',
      },
      extend: {
        title: 'Extend Vectors',
      },
      fitCurves: {
        title: 'Fit Curves to Vectors',
      },
      validator: {
        title: 'Vector Validator',
      },
      boundary: {
        title: 'Vector Boundary',
      },
      arrayCopy: {
        title: 'Array Copy',
      },
      nest: {
        title: 'Nest Parts',
      },
      plates: {
        title: 'Plate Production',
      },
      circularArray: {
        title: 'Circular Copy',
      },
      copyAlong: {
        title: 'Copy Along Vectors',
      },
    },
    vectorTexture: {
      title: 'Vector Texture',
    },
    text: {
      create: {
        title: 'Create Text',
      },
      selection: {
        title: 'Text Selection',
      },
      onCurve: {
        title: 'Text On A Curve',
      },
    },
    model: {
      sculpt: {
        title: 'Sculpting',
      },
      createShape: {
        title: 'Create Shape - Round',
      },
      createFlatShape: {
        title: 'Create Shape - Plane',
      },
      createAngledShape: {
        title: 'Create Shape - Angled',
      },
      createSmoothShape: {
        title: 'Create Shape - Smooth',
      },
      createCustomShape: {
        title: 'Create Shape - Custom',
      },
      importModel: {
        title: 'Orientate Model',
      },
      positionModel: {
        title: 'Position Model',
      },
      segmentModel: {
        title: '3D Segmenting',
      },
      sweep: {
        title: 'Two Rail Sweep',
      },
      extrude: {
        title: 'Extrude and Weave',
      },
      textureArea: {
        title: 'Create Texture Area',
      },
      turnSpin: {
        title: 'Turn and Spin',
      },
      edit: {
        title: 'Edit Component',
      },
      properties: {
        title: 'Component Properties',
      },
      boundary: {
        title: 'Component Boundary',
      },
      transform: {
        title: 'Transform Component',
      },
      exportStl: {
        title: 'Export Model',
      },
      replaceBelow: {
        title: 'Replace Below',
      },
      crossSection: {
        title: 'Create Cross Section Vector',
      },
      slice: {
        title: 'Slice Model',
      },
      refine: {
        smooth: {
          title: 'Smooth Components',
        },
        emboss: {
          title: 'Emboss Component',
        },
        scaleHeight: {
          title: 'Scale Model Height',
        },
        offset: {
          title: 'Offset Model',
        },
        draft: {
          title: 'Add Draft to Model',
        },
      },
    },
    toolpaths: {
      profile: {
        title: '2D Profile Toolpath',
      },
      pocket: {
        title: 'Pocket Toolpath',
      },
      drill: {
        title: 'Drilling Toolpath',
      },
      drillBank: {
        title: 'Drill Bank Toolpath',
      },
      vgroove: {
        title: 'V-Groove / Engraving Toolpath',
      },
      inlay: {
        title: 'VGroove Inlay Toolpath',
      },
      moulding: {
        title: 'Molding Toolpath',
      },
      engrave: {
        title: 'Quick Engrave',
      },
      chamfer: {
        title: 'Chamfer Toolpath',
      },
      arrayCopy: {
        title: 'Array Copy Toolpath',
      },
      straightInlay: {
        title: 'Inlay Toolpath',
      },
      thread: {
        title: 'Thread Milling Toolpath',
      },
      fluting: {
        title: 'Fluting Toolpath',
      },
      prism: {
        title: 'Prism Carving Toolpath',
      },
      texture: {
        title: 'Texture Toolpath',
      },
      material: {
        title: 'Material Setup',
      },
      save: {
        title: 'Save Toolpaths',
      },
      merge: {
        title: 'Create Merged Toolpath',
      },
      tiling: {
        title: 'Toolpath Tiling Manager',
      },
      photo: {
        title: 'Photo Carve',
      },
      preview: {
        title: 'Preview Toolpaths',
      },
      threeD: {
        roughTitle: 'Rough Machining Toolpath',
        finishTitle: 'Finish Machining Toolpath',
      },
      laser: {
        cutFillTitle: 'Laser Cut & Fill',
        pictureTitle: 'Laser Picture Toolpath',
      },
      sketch: {
        title: 'Sketch Carving Toolpath',
        laserTitle: 'Laser Sketch Engraving',
      },
    },
  },
  modelShell: {
    clearConfirm: 'Remove every component from the model? Undo brings them back.',
    menuCommands: {
      needsComponent: 'Select a component in the Components list first.',
      needsVectors: 'Select the vector to clear or split the component with.',
      needsPreview: 'Run a toolpath preview first.',
      oneBitmap: 'Select one bitmap, or nothing to pick an image file.',
      imageTitle: 'Create Component from Bitmap',
      imageFiles: 'Images',
    },
  },
  toolpathShell: {
    /** Undo label of recalculating several toolpaths at once. */
    recalculateUndo: 'Recalculate Toolpaths',
    recalculateAsk: (count: number) =>
      count === 1
        ? 'The change affects 1 toolpath. Recalculate it now?'
        : `The change affects ${count} toolpaths. Recalculate them now?`,
    crossingsAsk: (count: number) =>
      `The selected vectors cross each other or themselves in ${count} ${count === 1 ? 'place' : 'places'}, which can spoil the toolpath. Calculate anyway? Cancel opens the Vector Validator.`,
    crossingsAskOffset: (count: number) =>
      `The selected vectors cross each other or themselves in ${count} ${count === 1 ? 'place' : 'places'}, which can spoil the offset. Offset anyway? Cancel opens the Vector Validator.`,
    errorDeeper: 'Must be deeper than the start depth.',
    tooDeepItem: (name: string, depth: string, thickness: string) =>
      `${name} (${depth} deep, material ${thickness})`,
    tooDeep: (count: number, list: string, all: boolean) =>
      `${count === 1 ? 'A toolpath cuts' : `${count} toolpaths cut`} deeper below the material than set after the job change${all ? '' : ' (visible toolpaths checked)'}: ${list}.`,
    mergeOpen: 'Merge...',
    list: {
      label: 'Toolpath list',
      noFill: 'No Fill',
      heading: 'Toolpaths',
      headingSide: (side: string) => `Toolpaths (${side})`,
      allVisible: 'Show every toolpath in the list',
      visible: (name: string) => `Show ${name}`,
      stale: 'Needs recalculating',
      up: 'Move up',
      down: 'Move down',
      rename: 'Rename',
      renamePrompt: 'Toolpath name',
      empty: 'No toolpaths yet. Pick a toolpath operation above to calculate one.',
      time: (text: string) => `Estimated time ${text}`,
      estimateButton: 'Machining Time...',
      showThis: 'Show This',
      hideThis: 'Hide This',
      showOnly: 'Show Only This',
      showAllBut: 'Show All But This',
      showSameTool: 'Show All With This Tool',
      showAll: 'Show All',
      hideAll: 'Hide All',
      activateSheet: 'Activate Sheet',
      recalculateVisible: 'Recalculate Visible',
      recalculateEvery: 'Recalculate All',
      createGroup: 'Create an Empty Group',
      groupVisible: 'Group Visible',
      ungroup: 'Ungroup',
      moveToSheet: (name: string) => `Move to ${name}`,
      deleteThis: 'Delete This',
      deleteInvisible: 'Delete All Invisible',
      deleteVisible: 'Delete All Visible',
      deleteAll: 'Delete All',
      groupLabel: (name: string) => `Toolpath group ${name} menu`,
      listLabel: 'Toolpath list menu',
      expand: 'Expand',
      collapse: 'Collapse',
      toggleGroup: (name: string) => `Open or close ${name}`,
      groupVisibleLabel: (name: string) => `Show the toolpaths of ${name}`,
      showGroup: 'Show Toolpaths in Group',
      hideGroup: 'Hide Toolpaths in Group',
      recalculateGroup: 'Recalculate Group',
      deleteGroup: 'Delete Group...',
      deleteGroupToolpaths: (name: string) =>
        `Delete the toolpaths in ${name} too? Choose Cancel to keep them out of the group.`,
      sheetFilter: 'Sheet',
      allSheets: 'All Sheets',
      statusStale: 'Needs recalculating',
      statusEmpty: 'Calculated, but cuts nothing',
      statusOk: 'Calculated',
      toolIcon: (type: string) => `Tool: ${type}`,
    },
    imported: {
      title: 'Imported Toolpath',
      importTitle: 'Import Toolpaths',
      files: 'G-code files',
      imported: (count: number) =>
        count === 1 ? 'Imported 1 toolpath.' : `Imported ${count} toolpaths.`,
      skipped: (list: string) => `Some of the file was skipped: ${list}.`,
    },
  },
} as const;
