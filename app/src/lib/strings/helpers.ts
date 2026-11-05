// Helper pictures: what each explanatory drawing in a form shows, read out in
// place of the picture. The Job Setup and Material Setup pictures are
// described in strings/helpersJob.ts.

const sides = { outside: 'outside', inside: 'inside', on: 'on' } as const;
const directions = { climb: 'climb', conventional: 'conventional' } as const;

export const helpersStrings = {
  helpers: {
    drawing: {
      snap: {
        nodes: 'Snap to the nodes at the ends of each span',
        midpoints: 'Snap to the middle of each span',
        centres: 'Snap to the center of circles and arcs',
        intersections: 'Snap to where vectors cross',
        jobPoints: 'Snap to the corners and center of the job',
        guides: 'Snap to guide lines',
        grid: 'Snap to the grid points',
        smart: 'Smart snap: line up with nodes nearby',
      },
      corner: {
        none: 'Square corners',
        external: 'Rounded corners',
        internal: 'Corners cut inward with a radius',
      },
      arcMode: {
        threePoints: 'Arc through its start, a point on it and its end',
        centreStartEnd: 'Arc round its center, from its start to its end',
      },
      polygonSize: {
        radius: 'Polygon sized by the radius from its center to a corner',
        diameter: 'Polygon sized by the diameter across its corners',
      },
      star: 'Star sized by its outer radius, with its inner radius between the points',
      offset: {
        outwardRound: 'Offset outward, corners rounded',
        outwardSharp: 'Offset outward, corners kept sharp',
        inward: 'Offset inward',
      },
      fillet: {
        normal: 'Fillet: the corner rounded at the radius',
        dogbone: 'Dog-bone fillet: a round cut past the corner so a square part fits',
        tbone: 'T-bone fillet: a round cut along one edge past the corner',
        plasma: 'Plasma fillet: a loop round the corner so it stays sharp',
      },
      fit: {
        arcs: 'Fit curves with arcs through the points',
        beziers: 'Fit curves with smooth Bezier spans through the points',
        lines: 'Fit curves with straight lines through the points',
      },
      rotate: 'Rotate: the shape turns round its pivot point',
      circularArray: 'Circular copy: copies placed round a center',
      copyAlong: 'Copy along vectors: copies placed along a curve',
      nestCorner: {
        bottomLeft: 'Nesting starts at the bottom left corner of the sheet',
        bottomRight: 'Nesting starts at the bottom right corner of the sheet',
        topLeft: 'Nesting starts at the top left corner of the sheet',
        topRight: 'Nesting starts at the top right corner of the sheet',
      },
      distort: {
        box: 'Distort with a box: drag its corners and sides to bend the shapes',
        curve: 'Distort along one curve: the shapes follow the curve',
        twoCurves: 'Distort between two curves: the shapes stretch between them',
      },
      vectorTexture: {
        straight: 'Vector texture: parallel lines across the area',
        waves: 'Vector texture: wavy lines across the area',
      },
      unwrap: 'Vector unwrapper: vectors drawn on a cylinder laid out flat',
      trace: {
        lines: 'Trace bitmap: the picture outline traced with straight lines',
        curves: 'Trace bitmap: the picture outline traced with smooth curves',
      },
      textOnCurve: {
        above: 'Text above the curve',
        on: 'Text centered on the curve',
        below: 'Text below the curve',
      },
    },
    modeling: {
      combine: {
        add: 'Add: the shape sits on top of the model under it',
        subtract: 'Subtract: the shape is cut into the model under it',
        mergeHigh: 'Merge: where shapes overlap the higher one is kept',
        mergeLow: 'Low: where shapes overlap the lower one is kept',
        multiply: 'Multiply: the heights of the shapes are multiplied together',
      },
      shapeProfile: {
        round: 'Round shape: a domed cross section',
        flat: 'Flat shape: a flat top with straight sides',
        angled: 'Angled shape: sides that slope up to a ridge',
        smooth: 'Smooth shape: rounded shoulders and a flat top',
        custom: 'Custom shape: the cross section follows the profile vector',
      },
      heights: 'Heights: the shape on its base height, scaled to its height',
      turnSpin: {
        turn: 'Turn: the profile is swept round the rail',
        spin: 'Spin: the profile is spun round its center',
      },
      weaveOff: 'Extrude: the cross section runs along the rail',
      weaveOn: 'Weave: the strand passes over and under where rails cross',
      sweep: 'Two rail sweep: the cross sections run between the two rails',
      sculpt: {
        normal: 'Sculpt: smooth and smudge the surface',
        raise: 'Sculpt: raise the surface under the brush',
        lower: 'Sculpt: lower the surface under the brush',
      },
      draft: 'Draft: the walls of the relief lean at the draft angle',
      replaceBelow: 'Replace below: the model below the height is cut level',
      offsetModel: 'Offset: the model grown by the distance all round',
      scaleHeight: 'Scale model height: the model squashed or stretched to the height',
      slice: 'Slice: the model cut into layers of the thickness',
      zeroPlane: 'Zero plane: the plane through the model that sits on the material surface',
      rotaryWrap: {
        on: 'Wrapped: the model is bent round the rotary axis',
        off: 'Not wrapped: the model sits flat over the rotary axis',
      },
    },
    toolpaths: {
      tool: {
        endMill: 'End mill: a flat bottomed cutter of diameter D',
        ballNose: 'Ball nose: a cutter with a round tip of diameter D',
        vBit: 'V-bit: a pointed cutter with its included angle',
        engravingTool: 'Engraving tool: a pointed cutter with a small flat tip',
        taperedBallNose: 'Tapered ball nose: tapered sides and a round tip',
        radiusedEndMill: 'Radiused end mill: a flat bottom with rounded corners',
        formTool: 'Form tool: a cutter with its own profile',
        drill: 'Drill: a pointed drill of diameter D',
        diamondDrag: 'Diamond drag: a diamond point dragged across the surface',
        laserTool: 'Laser: a beam focused on the surface',
        threadMill: 'Thread mill: a cutter with thread teeth',
      },
      inlayKind: {
        maleStraight: 'Straight insert: a plug with straight sides',
        maleStepped: 'Stepped insert: a plug with a step round its base',
        femalePocket: 'Pocket: a recess the insert sits in',
        femaleHole: 'Hole: an opening cut right through for the insert',
      },
      laserCut: {
        outside: 'Laser cut outside the vector',
        inside: 'Laser cut inside the vector',
        on: 'Laser cut on the vector',
        hatch: 'Laser fill: the shape filled with hatch lines',
      },
      laserPicture: {
        raster: 'Laser picture: the image burned in lines of varying power',
        hatch: 'Laser picture: the image burned in crossing lines',
        vectors: 'Laser picture: the image burned along the selected vectors',
      },
      laserSketchHatch: 'Laser sketch fill: hatch lines one way',
      laserSketchCross: 'Laser sketch fill: hatch lines crossing both ways',
      cuttingDepths:
        'Cutting depths: the start depth D below the material surface and the cut depth C below it',
      vgrooveDepth: 'V-groove depth: a V groove below the start depth D',
      vgrooveFlatDepth:
        'V-groove depth: a V groove with a flat bottom at the flat depth F below the start depth D',
      machineVectors: (side: keyof typeof sides, direction: keyof typeof directions) =>
        `Machine vectors ${sides[side]}: the tool runs ${sides[side]} the vector, ${directions[direction]} cutting`,
      lastPass: 'Separate last pass: a wall of allowance A left for the final pass',
      tabs2d: 'Tabs: a flat bridge left in the cut with its length and thickness',
      tabs3d: 'Tabs: a bridge with sloped ends left in the cut with its length and thickness',
      ramp: {
        smooth: 'Smooth ramp: the tool slopes down along the cut',
        zigZag: 'Zig zag ramp: the tool slopes down and back along the cut',
        spiral: 'Spiral ramp: the tool slopes down round the whole loop',
      },
      lead: {
        none: 'No lead: the tool goes straight down on the vector',
        straight: 'Linear lead: the tool comes onto the vector along a straight line',
        arc: 'Arc lead: the tool comes onto the vector along an arc',
      },
      cornersRound: 'Corners: the tool rolls round an outside corner',
      cornersSharp: 'Corners: the tool runs out to a sharp outside corner',
      clearance: {
        offset: 'Offset clearance: loops offset inward from the pocket edge',
        raster: 'Raster clearance: parallel lines across the pocket',
      },
      drillPeck: 'Peck drilling: the drill goes down in steps and lifts out between them',
      drillStraight: 'Drilling: the drill goes down to the cut depth in one move',
      prism: {
        outside: 'Prism outside: a ridge rises from the vector edge to its center line',
        inside: 'Prism inside: a valley falls from the vector edge to its center line',
      },
      engrave: {
        outline: 'Engrave outline: a groove along the vector',
        fill: 'Engrave fill: grooves filling the shape',
      },
      chamfer: 'Chamfer: the edge cut at the tool angle, with the chamfer width and depth',
      thread: {
        internal: 'Internal thread: a thread cut inside a hole',
        external: 'External thread: a thread cut round a boss',
      },
      flute: {
        complete: 'Flute at full depth from start to end',
        start: 'Flute ramped in at its start',
        startEnd: 'Flute ramped at its start and its end',
      },
      mouldingPosition:
        'Molding: the cross section standing upright at the start of the drive rail',
      texturePattern: 'Texture: short random cuts across the area',
      photo: {
        raster: 'Photo carve: parallel grooves whose width draws the picture',
        hatch: 'Photo carve: crossing grooves whose width draws the picture',
        vectors: 'Photo carve: grooves along the selected vectors draw the picture',
      },
      strategy3d: {
        zLevelRaster: 'Z level roughing: the model cleared in flat steps',
        zLevelOffset: 'Z level roughing: the model cleared in flat steps of offset loops',
        raster3d: 'Raster roughing: lines over the model shape',
        raster: 'Raster finishing: lines across the model following its surface',
        rasterXY: 'Raster finishing in X and Y: lines both ways across the model',
        offset: 'Offset finishing: loops following the model outline',
      },
      boundary3d: {
        model: 'Machining limit: the model boundary',
        material: 'Machining limit: the whole material',
        selection: 'Machining limit: the selected vectors',
        level: 'Machining limit: the outline of the level',
      },
      inlayDepths: 'Inlay: the pocket and the plug pressed into it, with the gaps between them',
      toolpathGrid: 'Array copy: the toolpath repeated in rows and columns with spacing',
      tiling: 'Toolpath tiling: a long job cut in tiles that overlap',
    },
    furniture: {
      size: 'Size: the width, depth and height of the whole piece',
      thickness: 'Thickness: the stock the panels are cut from, measured across',
      fit: 'Fit: the clearance on each side of a part in its slot',
      explode: 'Explode: the pieces drawn apart the way they go together',
      verifyFit:
        'Verify Fit: the gap between two pieces against the target, give or take the tolerance',
    },
  },
} as const;
