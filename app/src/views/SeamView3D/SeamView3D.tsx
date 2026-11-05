import { useEffect, useRef, type JSX } from 'react';
import {
  AmbientLight,
  DirectionalLight,
  PerspectiveCamera,
  Scene,
  Sphere,
  Vector3,
  WebGLRenderer,
} from 'three';

import type { PreviewPanel, Vec3 } from '../../bridge/furniture';
import { disposeGroup, panelsGroup, placeCamera } from '../View3D/furnitureMeshes';

import './SeamView3D.scss';

interface SeamView3DProps {
  /** The two panels the seam joins, with their joint geometry. */
  panels: readonly PreviewPanel[];
  /** The seam's middle, furniture space: the view looks there. */
  focus: Vec3;
  label: string;
}

/**
 * The two panels of a seam drawn small in the seam popover, close in on
 * the seam: drag to turn them.
 */
export function SeamView3D({ panels, focus, label }: SeamView3DProps): JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const focusKey = focus.join(',');

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null || panels.length === 0) {
      return undefined;
    }
    let renderer: WebGLRenderer;
    try {
      renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true });
    } catch {
      return undefined;
    }
    renderer.setPixelRatio(window.devicePixelRatio);
    const scene = new Scene();
    scene.add(new AmbientLight(0xffffff, 1.2));
    const sun = new DirectionalLight(0xffffff, 1.6);
    sun.position.set(-1, -2, 3);
    scene.add(sun);
    const group = panelsGroup(panels);
    scene.add(group);
    // Close in on the seam: a sphere round its middle a few panel
    // thicknesses across, so the joint fills the picture.
    const thin = Math.min(
      ...panels.map((p) =>
        Math.min(...[0, 1, 2].map((k) => (p.box.max[k] ?? 0) - (p.box.min[k] ?? 0))),
      ),
    );
    const [fx, fy, fz] = focusKey.split(',').map(Number);
    const reach = Math.max(30, thin * 4);
    const sphere = new Sphere(new Vector3(fx, fy, fz), reach);
    const camera = new PerspectiveCamera(35, 1, 1, 100000);
    const turn = { azimuth: -Math.PI * 0.62, elevation: Math.PI / 6 };
    const draw = () => {
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      if (width === 0 || height === 0) {
        return;
      }
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      placeCamera(camera, sphere, turn.azimuth, turn.elevation);
      renderer.render(scene, camera);
    };
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
    draw();
    return () => {
      resize.disconnect();
      canvas.removeEventListener('pointerdown', down);
      canvas.removeEventListener('pointermove', move);
      canvas.removeEventListener('pointerup', up);
      disposeGroup(group);
      renderer.dispose();
    };
  }, [focusKey, panels]);

  return <canvas aria-label={label} className="seam-view-3d" ref={canvasRef} role="img" />;
}
