// Markers the 3D view draws where the preview checks found something: a
// small ball at each collision spot on the tool path or worst point of an
// area, colored by severity, drawn over the material so it always shows;
// the issue picked in the list gets a larger ball and a ring round it.

import { Group, Mesh, MeshBasicMaterial, RingGeometry, SphereGeometry, type Scene } from 'three';

import { SEVERITY_COLOURS } from './checkColours';

/** One marker: where it is in the scene and how severe. */
export interface IssueMarker {
  at: readonly [number, number, number];
  severity: keyof typeof SEVERITY_COLOURS;
}

/** How much larger the picked marker is. */
const PICKED_SCALE = 1.8;

export class IssueMarkers {
  private readonly group = new Group();
  private readonly ball = new SphereGeometry(1, 12, 8);
  private readonly ring = new RingGeometry(1.6, 2.1, 32);
  private readonly materials = new Map<string, MeshBasicMaterial>();
  private readonly ringMaterial = new MeshBasicMaterial({
    color: 0xffffff,
    depthTest: false,
    transparent: true,
    opacity: 0.9,
  });

  constructor(private readonly scene: Scene) {
    this.group.renderOrder = 12;
    scene.add(this.group);
  }

  private material(colour: string): MeshBasicMaterial {
    let material = this.materials.get(colour);
    if (material === undefined) {
      material = new MeshBasicMaterial({ color: colour, depthTest: false, transparent: true });
      this.materials.set(colour, material);
    }
    return material;
  }

  /** Shows `markers` with balls of `radius` (scene mm), `picked` larger. */
  set(markers: readonly IssueMarker[], picked: number | null, radius: number): void {
    this.group.clear();
    markers.forEach((marker, k) => {
      const mesh = new Mesh(this.ball, this.material(SEVERITY_COLOURS[marker.severity]));
      const scale = k === picked ? radius * PICKED_SCALE : radius;
      mesh.scale.setScalar(scale);
      mesh.position.set(marker.at[0], marker.at[1], marker.at[2]);
      mesh.renderOrder = 12;
      this.group.add(mesh);
      if (k === picked) {
        const ring = new Mesh(this.ring, this.ringMaterial);
        ring.scale.setScalar(scale);
        ring.position.copy(mesh.position);
        // Lying flat, seen from above.
        ring.rotation.x = -Math.PI / 2;
        ring.renderOrder = 13;
        this.group.add(ring);
      }
    });
  }

  /** How many markers show, for checks. */
  count(): number {
    return this.group.children.filter(
      (child) => child instanceof Mesh && child.geometry === this.ball,
    ).length;
  }

  dispose(): void {
    this.group.clear();
    this.scene.remove(this.group);
    this.ball.dispose();
    this.ring.dispose();
    this.ringMaterial.dispose();
    for (const material of this.materials.values()) {
      material.dispose();
    }
  }
}
