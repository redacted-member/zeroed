// Helper pictures of Job Setup, Material Setup and the Material Setup
// summary at the top of the Toolpaths panel. Imported directly by the job
// pictures, which load on demand; the other forms' pictures are described in
// strings/helpers.ts.

export const helpersJobStrings = {
  helpersJob: {
    jobType: {
      singleSided: 'Single sided job: one board cut from the top',
      doubleSided: 'Double sided job: the board is turned over to cut its other side',
      rotary: 'Rotary job: a cylinder turned by a rotary axis',
    },
    jobSize: 'Job size: the width X, height Y and thickness Z of the material',
    zZero: {
      top: 'Z zero on the material surface',
      bottom: 'Z zero on the machine bed, under the material',
    },
    rotaryZZero: {
      surface: 'Z zero on the cylinder surface',
      centre: 'Z zero on the center line of the cylinder',
    },
    xyDatum: {
      bottomLeft: 'XY datum at the bottom left corner of the material',
      bottomRight: 'XY datum at the bottom right corner of the material',
      topLeft: 'XY datum at the top left corner of the material',
      topRight: 'XY datum at the top right corner of the material',
      centre: 'XY datum at the center of the material',
    },
    resolution: 'Modeling resolution: a relief drawn with finer and finer points',
    rotaryAxis: {
      x: 'Rotary axis along X: the cylinder lies left to right',
      y: 'Rotary axis along Y: the cylinder lies front to back',
    },
    flip: {
      horizontal: 'Flip left to right: the board turns over about its vertical center line',
      vertical: 'Flip top to bottom: the board turns over about its horizontal center line',
    },
    modelPosition: 'Model position: the model inside the material with the gaps above and below',
    rapidGaps: 'Rapid Z gaps: clearance Z1 and plunge Z2 above the material',
    home: 'Home position: where the tool parks above the job',
  },
} as const;
