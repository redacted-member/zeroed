// The Assembly View's measurement in three.js: a line between the two
// points measured, a dot at each end and the distance written beside the
// line's middle, all drawn over the pieces at a constant size on screen.

import {
  BufferAttribute,
  BufferGeometry,
  CanvasTexture,
  Group,
  Line,
  LineBasicMaterial,
  Points,
  PointsMaterial,
  Sprite,
  SpriteMaterial,
} from 'three';

const MARK_COLOUR = 0x1f6feb;
/** The label's height on screen, as a share of the view's height. */
const LABEL_HEIGHT = 0.045;
const ORDER = 8;

export class MeasureMarks {
  private readonly group = new Group();
  private readonly lineMaterial = new LineBasicMaterial({
    color: MARK_COLOUR,
    depthTest: false,
    transparent: true,
  });
  private readonly pointMaterial = new PointsMaterial({
    color: MARK_COLOUR,
    size: 8,
    sizeAttenuation: false,
    depthTest: false,
    transparent: true,
  });
  private label: Sprite | null = null;

  constructor(parent: Group) {
    parent.add(this.group);
  }

  /** Shows a measurement from `a` to `b` (scene space) labelled `text`, or none. */
  set(
    ends: { a: [number, number, number]; b: [number, number, number]; text: string } | null,
  ): void {
    this.clear();
    if (ends === null) {
      return;
    }
    const positions = new Float32Array([...ends.a, ...ends.b]);
    const geometry = new BufferGeometry();
    geometry.setAttribute('position', new BufferAttribute(positions, 3));
    const line = new Line(geometry, this.lineMaterial);
    const dots = new Points(geometry, this.pointMaterial);
    line.renderOrder = ORDER;
    dots.renderOrder = ORDER;
    this.group.add(line, dots);
    const label = this.makeLabel(ends.text);
    if (label !== null) {
      label.position.set(
        (ends.a[0] + ends.b[0]) / 2,
        (ends.a[1] + ends.b[1]) / 2,
        (ends.a[2] + ends.b[2]) / 2,
      );
      this.label = label;
      this.group.add(label);
    }
  }

  private makeLabel(text: string): Sprite | null {
    const canvas = document.createElement('canvas');
    const context = canvas.getContext('2d');
    if (context === null) {
      return null;
    }
    const font = '600 40px system-ui, sans-serif';
    context.font = font;
    const width = Math.ceil(context.measureText(text).width) + 24;
    canvas.width = width;
    canvas.height = 56;
    context.font = font;
    context.fillStyle = 'rgba(255, 255, 255, 0.92)';
    context.fillRect(0, 0, width, 56);
    context.fillStyle = '#0d3b86';
    context.textBaseline = 'middle';
    context.fillText(text, 12, 29);
    const texture = new CanvasTexture(canvas);
    const sprite = new Sprite(
      new SpriteMaterial({ map: texture, depthTest: false, sizeAttenuation: false }),
    );
    sprite.scale.set((LABEL_HEIGHT * width) / 56, LABEL_HEIGHT, 1);
    sprite.center.set(-0.05, -0.2);
    sprite.renderOrder = ORDER + 1;
    return sprite;
  }

  private clear(): void {
    for (const child of [...this.group.children]) {
      this.group.remove(child);
      if (child instanceof Line || child instanceof Points) {
        child.geometry.dispose();
      }
    }
    if (this.label !== null) {
      this.label.material.map?.dispose();
      this.label.material.dispose();
      this.label = null;
    }
  }

  dispose(): void {
    this.clear();
    this.lineMaterial.dispose();
    this.pointMaterial.dispose();
  }
}
