// Every form the left panel can host, keyed by its id. Tool buttons open a
// form by id; the panel renders the matching component under its title.

import type { ComponentType, LazyExoticComponent } from 'react';

import { lazyNamed } from '../lib/lazyNamed';
import { strings } from '../lib/strings';
import type { FormId } from '../state/uiStore';

// Each form loads on first open, so the start bundle stays small.
const AddDraftForm = lazyNamed(() => import('./AddDraftForm/AddDraftForm'), 'AddDraftForm');
const AssemblyViewForm = lazyNamed(
  () => import('./AssemblyViewForm/AssemblyViewForm'),
  'AssemblyViewForm',
);
const FurnitureDesigner = lazyNamed(
  () => import('./FurnitureDesigner/FurnitureDesigner'),
  'FurnitureDesigner',
);
const AlignForm = lazyNamed(() => import('./AlignForm/AlignForm'), 'AlignForm');
const JointCreatorForm = lazyNamed(
  () => import('./JointCreatorForm/JointCreatorForm'),
  'JointCreatorForm',
);
const TextSelectionForm = lazyNamed(
  () => import('./TextSelectionForm/TextSelectionForm'),
  'TextSelectionForm',
);
const PlateProductionForm = lazyNamed(
  () => import('./PlateProductionForm/PlateProductionForm'),
  'PlateProductionForm',
);
const ArrayCopyForm = lazyNamed(() => import('./ArrayCopyForm/ArrayCopyForm'), 'ArrayCopyForm');
const BoundaryForm = lazyNamed(() => import('./BoundaryForm/BoundaryForm'), 'BoundaryForm');
const ChamferToolpathForm = lazyNamed(
  () => import('./ChamferToolpathForm/ChamferToolpathForm'),
  'ChamferToolpathForm',
);
const CircularArrayForm = lazyNamed(
  () => import('./CircularArrayForm/CircularArrayForm'),
  'CircularArrayForm',
);
const ComponentPropertiesForm = lazyNamed(
  () => import('../panels/ComponentProperties/ComponentProperties'),
  'ComponentPropertiesForm',
);
const ComponentEditForm = lazyNamed(
  () => import('./ComponentEditForm/ComponentEditForm'),
  'ComponentEditForm',
);
const ComponentTransformForm = lazyNamed(
  () => import('./ComponentTransformForm/ComponentTransformForm'),
  'ComponentTransformForm',
);
const CopyAlongForm = lazyNamed(() => import('./CopyAlongForm/CopyAlongForm'), 'CopyAlongForm');
const CreateShapeForm = lazyNamed(
  () => import('./CreateShapeForm/CreateShapeForm'),
  'CreateShapeForm',
);
const CreateFlatShapeForm = lazyNamed(
  () => import('./CreateShapeForm/CreateShapeForm'),
  'CreateFlatShapeForm',
);
const CreateAngledShapeForm = lazyNamed(
  () => import('./CreateShapeForm/CreateShapeForm'),
  'CreateAngledShapeForm',
);
const CreateSmoothShapeForm = lazyNamed(
  () => import('./CreateShapeForm/CreateShapeForm'),
  'CreateSmoothShapeForm',
);
const CreateCustomShapeForm = lazyNamed(
  () => import('./CreateShapeForm/CreateShapeForm'),
  'CreateCustomShapeForm',
);
const CreateTextForm = lazyNamed(() => import('./CreateTextForm/CreateTextForm'), 'CreateTextForm');
const CrossSectionForm = lazyNamed(
  () => import('./CrossSectionForm/CrossSectionForm'),
  'CrossSectionForm',
);
const DimensionsForm = lazyNamed(() => import('./DimensionsForm/DimensionsForm'), 'DimensionsForm');
const DistortForm = lazyNamed(() => import('./DistortForm/DistortForm'), 'DistortForm');
const DocumentVariablesForm = lazyNamed(
  () => import('./DocumentVariablesForm/DocumentVariablesForm'),
  'DocumentVariablesForm',
);
const DrawArcForm = lazyNamed(() => import('./DrawArcForm/DrawArcForm'), 'DrawArcForm');
const DrawCircleForm = lazyNamed(() => import('./DrawCircleForm/DrawCircleForm'), 'DrawCircleForm');
const DrawEllipseForm = lazyNamed(
  () => import('./DrawEllipseForm/DrawEllipseForm'),
  'DrawEllipseForm',
);
const DrawPolygonForm = lazyNamed(
  () => import('./DrawPolygonForm/DrawPolygonForm'),
  'DrawPolygonForm',
);
const DrawPolylineForm = lazyNamed(
  () => import('./DrawPolylineForm/DrawPolylineForm'),
  'DrawPolylineForm',
);
const DrawRectangleForm = lazyNamed(
  () => import('./DrawRectangleForm/DrawRectangleForm'),
  'DrawRectangleForm',
);
const DrawStarForm = lazyNamed(() => import('./DrawStarForm/DrawStarForm'), 'DrawStarForm');
const DrillToolpathForm = lazyNamed(
  () => import('./DrillToolpathForm/DrillToolpathForm'),
  'DrillToolpathForm',
);
const EditPictureForm = lazyNamed(
  () => import('./EditPictureForm/EditPictureForm'),
  'EditPictureForm',
);
const EmbossComponentForm = lazyNamed(
  () => import('./EmbossComponentForm/EmbossComponentForm'),
  'EmbossComponentForm',
);
const ExportStlForm = lazyNamed(() => import('./ExportStlForm/ExportStlForm'), 'ExportStlForm');
const ExtendForm = lazyNamed(() => import('./ExtendForm/ExtendForm'), 'ExtendForm');
const ExtrudeWeaveForm = lazyNamed(
  () => import('./ExtrudeWeaveForm/ExtrudeWeaveForm'),
  'ExtrudeWeaveForm',
);
const FilletForm = lazyNamed(() => import('./FilletForm/FilletForm'), 'FilletForm');
const Finish3dToolpathForm = lazyNamed(
  () => import('./Toolpath3dForm/Toolpath3dForm'),
  'Finish3dToolpathForm',
);
const FitCurvesForm = lazyNamed(() => import('./FitCurvesForm/FitCurvesForm'), 'FitCurvesForm');
const FlutingToolpathForm = lazyNamed(
  () => import('./FlutingToolpathForm/FlutingToolpathForm'),
  'FlutingToolpathForm',
);
const FreehandForm = lazyNamed(() => import('./FreehandForm/FreehandForm'), 'FreehandForm');
const GadgetForm = lazyNamed(() => import('./GadgetForm/GadgetForm'), 'GadgetForm');
const GuidePositionForm = lazyNamed(
  () => import('./GuidePositionForm/GuidePositionForm'),
  'GuidePositionForm',
);
const InlayToolpathForm = lazyNamed(
  () => import('./InlayToolpathForm/InlayToolpathForm'),
  'InlayToolpathForm',
);
const JobSetupForm = lazyNamed(() => import('./JobSetupForm/JobSetupForm'), 'JobSetupForm');
const JoinForm = lazyNamed(() => import('./JoinForm/JoinForm'), 'JoinForm');
const LaserCutFillForm = lazyNamed(
  () => import('./LaserCutFillForm/LaserCutFillForm'),
  'LaserCutFillForm',
);
const LaserPictureForm = lazyNamed(
  () => import('./LaserPictureForm/LaserPictureForm'),
  'LaserPictureForm',
);
const LaserSketchForm = lazyNamed(
  () => import('./LaserSketchForm/LaserSketchForm'),
  'LaserSketchForm',
);
const MaterialSetupForm = lazyNamed(
  () => import('./MaterialSetupForm/MaterialSetupForm'),
  'MaterialSetupForm',
);
const MeasureForm = lazyNamed(() => import('./MeasureForm/MeasureForm'), 'MeasureForm');
const ArrayCopyToolpathForm = lazyNamed(
  () => import('./ArrayCopyToolpathForm/ArrayCopyToolpathForm'),
  'ArrayCopyToolpathForm',
);
const MergeToolpathsForm = lazyNamed(
  () => import('./MergeToolpathsForm/MergeToolpathsForm'),
  'MergeToolpathsForm',
);
const MirrorForm = lazyNamed(() => import('./MirrorForm/MirrorForm'), 'MirrorForm');
const MouldingToolpathForm = lazyNamed(
  () => import('./MouldingToolpathForm/MouldingToolpathForm'),
  'MouldingToolpathForm',
);
const MoveForm = lazyNamed(() => import('./MoveForm/MoveForm'), 'MoveForm');
const NestForm = lazyNamed(() => import('./NestForm/NestForm'), 'NestForm');
const OffsetForm = lazyNamed(() => import('./OffsetForm/OffsetForm'), 'OffsetForm');
const OffsetModelForm = lazyNamed(
  () => import('./OffsetModelForm/OffsetModelForm'),
  'OffsetModelForm',
);
const OrientateModelForm = lazyNamed(
  () => import('./OrientateModelForm/OrientateModelForm'),
  'OrientateModelForm',
);
const PocketToolpathForm = lazyNamed(
  () => import('./PocketToolpathForm/PocketToolpathForm'),
  'PocketToolpathForm',
);
const PositionModelForm = lazyNamed(
  () => import('./PositionModelForm/PositionModelForm'),
  'PositionModelForm',
);
const PreviewToolpathsForm = lazyNamed(
  () => import('./PreviewToolpathsForm/PreviewToolpathsForm'),
  'PreviewToolpathsForm',
);
const PrismToolpathForm = lazyNamed(
  () => import('./PrismToolpathForm/PrismToolpathForm'),
  'PrismToolpathForm',
);
const ProfileToolpathForm = lazyNamed(
  () => import('./ProfileToolpathForm/ProfileToolpathForm'),
  'ProfileToolpathForm',
);
const QuickEngraveToolpathForm = lazyNamed(
  () => import('./QuickEngraveToolpathForm/QuickEngraveToolpathForm'),
  'QuickEngraveToolpathForm',
);
const ReplaceBelowForm = lazyNamed(
  () => import('./ReplaceBelowForm/ReplaceBelowForm'),
  'ReplaceBelowForm',
);
const RotateForm = lazyNamed(() => import('./RotateForm/RotateForm'), 'RotateForm');
const Rough3dToolpathForm = lazyNamed(
  () => import('./Toolpath3dForm/Toolpath3dForm'),
  'Rough3dToolpathForm',
);
const SaveToolpathsForm = lazyNamed(
  () => import('./SaveToolpathsForm/SaveToolpathsForm'),
  'SaveToolpathsForm',
);
const PhotoCarveToolpathForm = lazyNamed(
  () => import('./PhotoCarveToolpathForm/PhotoCarveToolpathForm'),
  'PhotoCarveToolpathForm',
);
const ImportedToolpathForm = lazyNamed(
  () => import('./ImportedToolpathForm/ImportedToolpathForm'),
  'ImportedToolpathForm',
);
const ToolpathTilingForm = lazyNamed(
  () => import('./ToolpathTilingForm/ToolpathTilingForm'),
  'ToolpathTilingForm',
);
const ScaleModelHeightForm = lazyNamed(
  () => import('./ScaleModelHeightForm/ScaleModelHeightForm'),
  'ScaleModelHeightForm',
);
const SculptForm = lazyNamed(() => import('./SculptForm/SculptForm'), 'SculptForm');
const SegmentModelForm = lazyNamed(
  () => import('./SegmentModelForm/SegmentModelForm'),
  'SegmentModelForm',
);
const SetSizeForm = lazyNamed(() => import('./SetSizeForm/SetSizeForm'), 'SetSizeForm');
const SketchToolpathForm = lazyNamed(
  () => import('./SketchToolpathForm/SketchToolpathForm'),
  'SketchToolpathForm',
);
const SliceModelForm = lazyNamed(() => import('./SliceModelForm/SliceModelForm'), 'SliceModelForm');
const SmoothComponentsForm = lazyNamed(
  () => import('./SmoothComponentsForm/SmoothComponentsForm'),
  'SmoothComponentsForm',
);
const SnapSettingsForm = lazyNamed(
  () => import('./SnapSettingsForm/SnapSettingsForm'),
  'SnapSettingsForm',
);
const StraightInlayToolpathForm = lazyNamed(
  () => import('./StraightInlayToolpathForm/StraightInlayToolpathForm'),
  'StraightInlayToolpathForm',
);
const TextOnCurveForm = lazyNamed(
  () => import('./TextOnCurveForm/TextOnCurveForm'),
  'TextOnCurveForm',
);
const TextureAreaForm = lazyNamed(
  () => import('./TextureAreaForm/TextureAreaForm'),
  'TextureAreaForm',
);
const TextureToolpathForm = lazyNamed(
  () => import('./TextureToolpathForm/TextureToolpathForm'),
  'TextureToolpathForm',
);
const DrillBankToolpathForm = lazyNamed(
  () => import('./DrillBankToolpathForm/DrillBankToolpathForm'),
  'DrillBankToolpathForm',
);
const ThreadMillingToolpathForm = lazyNamed(
  () => import('./ThreadMillingToolpathForm/ThreadMillingToolpathForm'),
  'ThreadMillingToolpathForm',
);
const TraceBitmapForm = lazyNamed(
  () => import('./TraceBitmapForm/TraceBitmapForm'),
  'TraceBitmapForm',
);
const TrimForm = lazyNamed(() => import('./PickToolForm/PickToolForm'), 'TrimForm');
const TrimObjectsForm = lazyNamed(
  () => import('./TrimObjectsForm/TrimObjectsForm'),
  'TrimObjectsForm',
);
const TurnSpinForm = lazyNamed(() => import('./TurnSpinForm/TurnSpinForm'), 'TurnSpinForm');
const TwoRailSweepForm = lazyNamed(
  () => import('./TwoRailSweepForm/TwoRailSweepForm'),
  'TwoRailSweepForm',
);
const VGrooveToolpathForm = lazyNamed(
  () => import('./VGrooveToolpathForm/VGrooveToolpathForm'),
  'VGrooveToolpathForm',
);
const ValidatorForm = lazyNamed(() => import('./ValidatorForm/ValidatorForm'), 'ValidatorForm');
const VectorTextureForm = lazyNamed(
  () => import('./VectorTextureForm/VectorTextureForm'),
  'VectorTextureForm',
);
const VectorUnwrapperForm = lazyNamed(
  () => import('./VectorUnwrapperForm/VectorUnwrapperForm'),
  'VectorUnwrapperForm',
);

export interface FormEntry {
  title: string;
  Component: LazyExoticComponent<ComponentType>;
}

export const FORMS: Record<FormId, FormEntry> = {
  documentVariables: {
    title: strings.formTitles.documentVariables.title,
    Component: DocumentVariablesForm,
  },
  jobSetup: { title: strings.formTitles.jobSetup.title, Component: JobSetupForm },
  drawRectangle: {
    title: strings.formTitles.createVectors.rectangle.title,
    Component: DrawRectangleForm,
  },
  drawCircle: { title: strings.formTitles.createVectors.circle.title, Component: DrawCircleForm },
  drawEllipse: {
    title: strings.formTitles.createVectors.ellipse.title,
    Component: DrawEllipseForm,
  },
  drawPolygon: {
    title: strings.formTitles.createVectors.polygon.title,
    Component: DrawPolygonForm,
  },
  drawStar: { title: strings.formTitles.createVectors.star.title, Component: DrawStarForm },
  drawPolyline: {
    title: strings.formTitles.createVectors.polyline.title,
    Component: DrawPolylineForm,
  },
  drawArc: { title: strings.formTitles.createVectors.arc.title, Component: DrawArcForm },
  dimensions: {
    title: strings.formTitles.createVectors.dimensions.title,
    Component: DimensionsForm,
  },
  drawFreehand: { title: strings.formTitles.drawTools.freehand.title, Component: FreehandForm },
  traceBitmap: {
    title: strings.formTitles.createVectors.traceBitmap.title,
    Component: TraceBitmapForm,
  },
  move: { title: strings.formTitles.transform.move.title, Component: MoveForm },
  setSize: { title: strings.formTitles.transform.setSize.title, Component: SetSizeForm },
  rotate: { title: strings.formTitles.transform.rotate.title, Component: RotateForm },
  mirror: { title: strings.formTitles.transform.mirror.title, Component: MirrorForm },
  distort: { title: strings.formTitles.transform.distort.title, Component: DistortForm },
  vectorUnwrapper: {
    title: strings.formTitles.vectorUnwrapper.title,
    Component: VectorUnwrapperForm,
  },
  align: { title: strings.formTitles.transform.align.title, Component: AlignForm },
  snapSettings: { title: strings.formTitles.snapSettings.title, Component: SnapSettingsForm },
  sculpt: { title: strings.formTitles.model.sculpt.title, Component: SculptForm },
  guidePosition: { title: strings.formTitles.guidePosition.title, Component: GuidePositionForm },
  join: { title: strings.formTitles.editObjects.join.title, Component: JoinForm },
  offset: { title: strings.formTitles.editObjects.offset.title, Component: OffsetForm },
  fillet: { title: strings.formTitles.editObjects.fillet.title, Component: FilletForm },
  trim: { title: strings.formTitles.editObjects.trim.title, Component: TrimForm },
  trimObjects: {
    title: strings.formTitles.editObjects.trimObjects.title,
    Component: TrimObjectsForm,
  },
  measure: { title: strings.formTitles.drawTools.measure.title, Component: MeasureForm },
  editPicture: { title: strings.picture.edit.title, Component: EditPictureForm },
  vectorTexture: { title: strings.formTitles.vectorTexture.title, Component: VectorTextureForm },
  extend: { title: strings.formTitles.editObjects.extend.title, Component: ExtendForm },
  fitCurves: { title: strings.formTitles.editObjects.fitCurves.title, Component: FitCurvesForm },
  validator: { title: strings.formTitles.editObjects.validator.title, Component: ValidatorForm },
  boundary: { title: strings.formTitles.editObjects.boundary.title, Component: BoundaryForm },
  arrayCopy: { title: strings.formTitles.editObjects.arrayCopy.title, Component: ArrayCopyForm },
  nest: { title: strings.formTitles.editObjects.nest.title, Component: NestForm },
  plates: { title: strings.formTitles.editObjects.plates.title, Component: PlateProductionForm },
  gadget: { title: strings.gadgets.formTitle, Component: GadgetForm },
  circularArray: {
    title: strings.formTitles.editObjects.circularArray.title,
    Component: CircularArrayForm,
  },
  copyAlong: { title: strings.formTitles.editObjects.copyAlong.title, Component: CopyAlongForm },
  createText: { title: strings.formTitles.text.create.title, Component: CreateTextForm },
  textSelection: { title: strings.formTitles.text.selection.title, Component: TextSelectionForm },
  textOnCurve: { title: strings.formTitles.text.onCurve.title, Component: TextOnCurveForm },
  profileToolpath: {
    title: strings.formTitles.toolpaths.profile.title,
    Component: ProfileToolpathForm,
  },
  pocketToolpath: {
    title: strings.formTitles.toolpaths.pocket.title,
    Component: PocketToolpathForm,
  },
  drillToolpath: { title: strings.formTitles.toolpaths.drill.title, Component: DrillToolpathForm },
  vgrooveToolpath: {
    title: strings.formTitles.toolpaths.vgroove.title,
    Component: VGrooveToolpathForm,
  },
  inlayToolpath: { title: strings.formTitles.toolpaths.inlay.title, Component: InlayToolpathForm },
  mouldingToolpath: {
    title: strings.formTitles.toolpaths.moulding.title,
    Component: MouldingToolpathForm,
  },
  quickEngraveToolpath: {
    title: strings.formTitles.toolpaths.engrave.title,
    Component: QuickEngraveToolpathForm,
  },
  chamferToolpath: {
    title: strings.formTitles.toolpaths.chamfer.title,
    Component: ChamferToolpathForm,
  },
  straightInlayToolpath: {
    title: strings.formTitles.toolpaths.straightInlay.title,
    Component: StraightInlayToolpathForm,
  },
  drillBankToolpath: {
    title: strings.formTitles.toolpaths.drillBank.title,
    Component: DrillBankToolpathForm,
  },
  threadMillingToolpath: {
    title: strings.formTitles.toolpaths.thread.title,
    Component: ThreadMillingToolpathForm,
  },
  flutingToolpath: {
    title: strings.formTitles.toolpaths.fluting.title,
    Component: FlutingToolpathForm,
  },
  prismToolpath: { title: strings.formTitles.toolpaths.prism.title, Component: PrismToolpathForm },
  textureToolpath: {
    title: strings.formTitles.toolpaths.texture.title,
    Component: TextureToolpathForm,
  },
  materialSetup: {
    title: strings.formTitles.toolpaths.material.title,
    Component: MaterialSetupForm,
  },
  saveToolpaths: { title: strings.formTitles.toolpaths.save.title, Component: SaveToolpathsForm },
  mergeToolpaths: {
    title: strings.formTitles.toolpaths.merge.title,
    Component: MergeToolpathsForm,
  },
  arrayCopyToolpath: {
    title: strings.formTitles.toolpaths.arrayCopy.title,
    Component: ArrayCopyToolpathForm,
  },
  toolpathTiling: {
    title: strings.formTitles.toolpaths.tiling.title,
    Component: ToolpathTilingForm,
  },
  photoCarveToolpath: {
    title: strings.formTitles.toolpaths.photo.title,
    Component: PhotoCarveToolpathForm,
  },
  importedToolpath: {
    title: strings.toolpathShell.imported.title,
    Component: ImportedToolpathForm,
  },
  previewToolpaths: {
    title: strings.formTitles.toolpaths.preview.title,
    Component: PreviewToolpathsForm,
  },
  createFlatShape: {
    title: strings.formTitles.model.createFlatShape.title,
    Component: CreateFlatShapeForm,
  },
  createShape: { title: strings.formTitles.model.createShape.title, Component: CreateShapeForm },
  createAngledShape: {
    title: strings.formTitles.model.createAngledShape.title,
    Component: CreateAngledShapeForm,
  },
  createSmoothShape: {
    title: strings.formTitles.model.createSmoothShape.title,
    Component: CreateSmoothShapeForm,
  },
  createCustomShape: {
    title: strings.formTitles.model.createCustomShape.title,
    Component: CreateCustomShapeForm,
  },
  importModel: { title: strings.formTitles.model.importModel.title, Component: OrientateModelForm },
  positionModel: {
    title: strings.formTitles.model.positionModel.title,
    Component: PositionModelForm,
  },
  segmentModel: { title: strings.formTitles.model.segmentModel.title, Component: SegmentModelForm },
  twoRailSweep: { title: strings.formTitles.model.sweep.title, Component: TwoRailSweepForm },
  extrudeWeave: { title: strings.formTitles.model.extrude.title, Component: ExtrudeWeaveForm },
  textureArea: { title: strings.formTitles.model.textureArea.title, Component: TextureAreaForm },
  turnSpin: { title: strings.formTitles.model.turnSpin.title, Component: TurnSpinForm },
  componentEdit: { title: strings.formTitles.model.edit.title, Component: ComponentEditForm },
  componentProperties: {
    title: strings.formTitles.model.properties.title,
    Component: ComponentPropertiesForm,
  },
  componentTransform: {
    title: strings.formTitles.model.transform.title,
    Component: ComponentTransformForm,
  },
  exportStl: { title: strings.formTitles.model.exportStl.title, Component: ExportStlForm },
  replaceBelow: { title: strings.formTitles.model.replaceBelow.title, Component: ReplaceBelowForm },
  crossSection: { title: strings.formTitles.model.crossSection.title, Component: CrossSectionForm },
  sliceModel: { title: strings.formTitles.model.slice.title, Component: SliceModelForm },
  smoothComponents: {
    title: strings.formTitles.model.refine.smooth.title,
    Component: SmoothComponentsForm,
  },
  embossComponent: {
    title: strings.formTitles.model.refine.emboss.title,
    Component: EmbossComponentForm,
  },
  scaleModelHeight: {
    title: strings.formTitles.model.refine.scaleHeight.title,
    Component: ScaleModelHeightForm,
  },
  offsetModel: { title: strings.formTitles.model.refine.offset.title, Component: OffsetModelForm },
  addDraft: { title: strings.formTitles.model.refine.draft.title, Component: AddDraftForm },
  rough3dToolpath: {
    title: strings.formTitles.toolpaths.threeD.roughTitle,
    Component: Rough3dToolpathForm,
  },
  finish3dToolpath: {
    title: strings.formTitles.toolpaths.threeD.finishTitle,
    Component: Finish3dToolpathForm,
  },
  laserCutFillToolpath: {
    title: strings.formTitles.toolpaths.laser.cutFillTitle,
    Component: LaserCutFillForm,
  },
  sketchToolpath: {
    title: strings.formTitles.toolpaths.sketch.title,
    Component: SketchToolpathForm,
  },
  laserSketchToolpath: {
    title: strings.formTitles.toolpaths.sketch.laserTitle,
    Component: LaserSketchForm,
  },
  laserPictureToolpath: {
    title: strings.formTitles.toolpaths.laser.pictureTitle,
    Component: LaserPictureForm,
  },
  assemblyView: { title: strings.formTitles.assemblyView.title, Component: AssemblyViewForm },
  jointCreator: { title: strings.formTitles.joints.title, Component: JointCreatorForm },
  furnitureDesigner: {
    title: strings.formTitles.furnitureDesigner.title,
    Component: FurnitureDesigner,
  },
};
