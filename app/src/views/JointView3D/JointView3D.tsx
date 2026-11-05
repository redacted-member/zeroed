import { useEffect, useRef, type JSX } from 'react';
import {
  AmbientLight,
  Box3,
  Color,
  DirectionalLight,
  EdgesGeometry,
  ExtrudeGeometry,
  Group,
  LineBasicMaterial,
  LineSegments,
  Matrix4,
  Mesh,
  MeshStandardMaterial,
  Path as ShapePath,
  PerspectiveCamera,
  Scene,
  Shape,
  Sphere,
  Vector2,
  Vector3,
  WebGLRenderer,
} from 'three';

import type { PartSolid, XY } from '../../bridge/joints';

import './JointView3D.scss';

interface JointView3DProps {
  a: PartSolid;
  b: PartSolid;
  /** B's part space to A's, column major 4 by 4. */
  bToA: number[];
  /** 0 together, 1 pulled apart. */
  explode: number;
  label: string;
}

const A_COLOR = new Color(0xd9b98c);
const B_COLOR = new Color(0xa8784a);
const EDGE_COLOR = 0x3b2a1a;

/** The part as a solid: its rings extruded by its thickness, Z up. */
function partGroup(part: PartSolid, color: Color): Group {
  const group = new Group();
  const [outer, ...holes] = [...part.rings].sort((x, y) => Math.abs(area(y)) - Math.abs(area(x)));
  if (outer === undefined) {
    return group;
  }
  const shape = new Shape(outer.map(([x, y]) => new Vector2(x, y)));
  for (const hole of holes) {
    shape.holes.push(new ShapePath(hole.map(([x, y]) => new Vector2(x, y))));
  }
  const geometry = new ExtrudeGeometry(shape, {
    depth: part.thickness,
    bevelEnabled: false,
    curveSegments: 8,
  });
  const mesh = new Mesh(
    geometry,
    new MeshStandardMaterial({ color, roughness: 0.8, metalness: 0 }),
  );
  const edges = new LineSegments(
    new EdgesGeometry(geometry, 30),
    new LineBasicMaterial({ color: EDGE_COLOR }),
  );
  group.add(mesh, edges);
  return group;
}

function area(ring: XY[]): number {
  let sum = 0;
  for (let i = 0; i < ring.length; i += 1) {
    const [x0, y0] = ring[i];
    const [x1, y1] = ring[(i + 1) % ring.length];
    sum += x0 * y1 - x1 * y0;
  }
  return sum / 2;
}

function dispose(group: Group): void {
  group.traverse((object) => {
    if (object instanceof Mesh || object instanceof LineSegments) {
      object.geometry.dispose();
      const material = object.material as MeshStandardMaterial | LineBasicMaterial;
      material.dispose();
    }
  });
}

/**
 * The two parts of a joint put together, drawn small in the Joint Creator
 * panel: drag to turn it, the Explode slider pulls the second part out.
 */
export function JointView3D({ a, b, bToA, explode, label }: JointView3DProps): JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const viewRef = useRef<{
    renderer: WebGLRenderer;
    scene: Scene;
    camera: PerspectiveCamera;
    turn: { azimuth: number; elevation: number };
    sphere: Sphere;
    draw: () => void;
  } | null>(null);
  const partsRef = useRef<{ a: Group; b: Group; out: Vector3; place: Matrix4 } | null>(null);

  // The renderer, scene and lights, once.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null) {
      return undefined;
    }
    const renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    const scene = new Scene();
    scene.add(new AmbientLight(0xffffff, 1.2));
    const sun = new DirectionalLight(0xffffff, 1.6);
    sun.position.set(1, -2, 3);
    scene.add(sun);
    const camera = new PerspectiveCamera(35, 1, 1, 100000);
    camera.up.set(0, 0, 1);
    const turn = { azimuth: -Math.PI / 3, elevation: Math.PI / 5 };
    const sphere = new Sphere(new Vector3(), 100);
    const draw = () => {
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      if (width === 0 || height === 0) {
        return;
      }
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      const distance = (sphere.radius / Math.sin((camera.fov * Math.PI) / 360)) * 1.1;
      const e = turn.elevation;
      camera.position.set(
        sphere.center.x + distance * Math.cos(e) * Math.cos(turn.azimuth),
        sphere.center.y + distance * Math.cos(e) * Math.sin(turn.azimuth),
        sphere.center.z + distance * Math.sin(e),
      );
      camera.near = Math.max(distance / 100, 0.1);
      camera.far = distance * 10;
      camera.lookAt(sphere.center);
      camera.updateProjectionMatrix();
      renderer.render(scene, camera);
    };
    viewRef.current = { renderer, scene, camera, turn, sphere, draw };
    let drag: { x: number; y: number } | null = null;
    const down = (event: PointerEvent) => {
      drag = { x: event.clientX, y: event.clientY };
      canvas.setPointerCapture(event.pointerId);
    };
    const move = (event: PointerEvent) => {
      if (drag === null) {
        return;
      }
      turn.azimuth -= (event.clientX - drag.x) * 0.01;
      turn.elevation = Math.min(
        Math.PI / 2 - 0.01,
        Math.max(-Math.PI / 2 + 0.01, turn.elevation + (event.clientY - drag.y) * 0.01),
      );
      drag = { x: event.clientX, y: event.clientY };
      draw();
    };
    const up = () => {
      drag = null;
    };
    canvas.addEventListener('pointerdown', down);
    canvas.addEventListener('pointermove', move);
    canvas.addEventListener('pointerup', up);
    const resize = new ResizeObserver(draw);
    resize.observe(canvas);
    return () => {
      resize.disconnect();
      canvas.removeEventListener('pointerdown', down);
      canvas.removeEventListener('pointermove', move);
      canvas.removeEventListener('pointerup', up);
      renderer.dispose();
      viewRef.current = null;
    };
  }, []);

  // The parts, whenever the joint changes.
  useEffect(() => {
    const view = viewRef.current;
    if (view === null) {
      return undefined;
    }
    const groupA = partGroup(a, A_COLOR);
    const groupB = partGroup(b, B_COLOR);
    const place = new Matrix4().fromArray(bToA);
    groupB.matrixAutoUpdate = false;
    groupB.matrix.copy(place);
    view.scene.add(groupA, groupB);
    groupB.updateMatrixWorld(true);
    const boxA = new Box3().setFromObject(groupA);
    const boxB = new Box3().setFromObject(groupB);
    const out = boxB.getCenter(new Vector3()).sub(boxA.getCenter(new Vector3()));
    if (out.lengthSq() < 1e-6) {
      out.set(0, 0, 1);
    }
    out.normalize().multiplyScalar(Math.max(boxA.getSize(new Vector3()).length() * 0.4, 20));
    boxA.union(boxB).getBoundingSphere(view.sphere);
    view.sphere.radius *= 1 + 0.4;
    partsRef.current = { a: groupA, b: groupB, out, place };
    view.draw();
    return () => {
      view.scene.remove(groupA, groupB);
      dispose(groupA);
      dispose(groupB);
      partsRef.current = null;
    };
  }, [a, b, bToA]);

  // The explode, without building the parts again.
  useEffect(() => {
    const parts = partsRef.current;
    const view = viewRef.current;
    if (parts === null || view === null) {
      return;
    }
    const shift = new Matrix4().makeTranslation(parts.out.clone().multiplyScalar(explode));
    parts.b.matrix.copy(shift.multiply(parts.place));
    parts.b.updateMatrixWorld(true);
    view.draw();
  }, [explode, a, b, bToA]);

  return <canvas aria-label={label} className="joint-view-3d" role="img" ref={canvasRef} />;
}
