import { useEffect, useRef } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import {
  CSS2DRenderer,
  CSS2DObject,
} from 'three/examples/jsm/renderers/CSS2DRenderer.js';
import type { GraphState, RoomNode } from './types';

interface Props {
  state: GraphState;
}

/**
 * Numeric color palette for the 3D scene, mirrored from the single token source
 * (design-system/tokens.css). Three.js needs numeric hex, so we read the `--wb-*`
 * custom properties once via getComputedStyle and convert through THREE.Color —
 * no second palette, the 2D canvas and this preview track the same tokens.
 * `exit`/`poi` have no token equivalent and stay 3D-local accents (noted below).
 */
interface Palette {
  bg: number;          // --wb-bg-canvas (scene background + fog)
  text: string;        // --wb-text (CSS string for CSS2D node labels)
  nodeBoss: number;    // --wb-error
  nodeTreasure: number;// --wb-warning
  nodeStart: number;   // --wb-success
  nodeExit: number;    // cyan — 3D-local accent, no token equivalent
  nodePoi: number;     // teal — 3D-local accent, no token equivalent
  nodeDefault: number; // --wb-text-muted
  edgeBi: number;      // --wb-edge-bi (bidirectional, mirrors 2D canvas)
  edgeUni: number;     // --wb-edge-uni (one-way, mirrors 2D canvas)
  gridMajor: number;   // --wb-border-strong (grid floor major lines)
  gridMinor: number;   // --wb-border (grid floor minor lines)
  scrim: string;       // --wb-bg-canvas @ 0.85 (CSS string, edge-label backdrop)
}

/** Read the design tokens once and convert to a numeric Three.js palette. */
function readPalette(): Palette {
  const css = getComputedStyle(document.documentElement);
  const num = (name: string) => new THREE.Color(css.getPropertyValue(name).trim()).getHex();
  const str = (name: string) => css.getPropertyValue(name).trim();
  const canvas = new THREE.Color(css.getPropertyValue('--wb-bg-canvas').trim());
  const rgb = (c: THREE.Color) => `${Math.round(c.r * 255)}, ${Math.round(c.g * 255)}, ${Math.round(c.b * 255)}`;
  return {
    bg: num('--wb-bg-canvas'),
    text: str('--wb-text'),
    nodeBoss: num('--wb-error'),
    nodeTreasure: num('--wb-warning'),
    nodeStart: num('--wb-success'),
    nodeExit: 0x06b6d4, // cyan — 3D-local accent (no token equivalent)
    nodePoi: 0x14b8a6,  // teal — 3D-local accent (no token equivalent)
    nodeDefault: num('--wb-text-muted'),
    edgeBi: num('--wb-edge-bi'),
    edgeUni: num('--wb-edge-uni'),
    gridMajor: num('--wb-border-strong'),
    gridMinor: num('--wb-border'),
    scrim: `rgba(${rgb(canvas)}, 0.85)`,
  };
}

/** Numeric Three.js color → CSS hex string (for CSS2D label text). */
const hexCss = (n: number) => `#${n.toString(16).padStart(6, '0')}`;

/** Pick node color based on marks / POI presence (mapped to design tokens). */
function nodeColor(room: RoomNode, p: Palette): number {
  const marks = room.marks.map((m) => m.toLowerCase());
  const hasPois = room.pois.length > 0;

  // Boss / enemy rooms → error
  if (marks.some((m) => m.includes('boss') || m.includes('enemy'))) return p.nodeBoss;
  // Treasure / loot rooms → warning
  if (marks.some((m) => m.includes('treasure') || m.includes('loot') || m.includes('item'))) return p.nodeTreasure;
  // Start / spawn rooms → success
  if (marks.some((m) => m.includes('start') || m.includes('spawn') || m.includes('entrance'))) return p.nodeStart;
  // Exit / goal rooms → cyan accent
  if (marks.some((m) => m.includes('exit') || m.includes('goal') || m.includes('end'))) return p.nodeExit;
  // Occupied rooms (has POIs) → teal accent
  if (hasPois) return p.nodePoi;
  // Default → muted
  return p.nodeDefault;
}

/** Build a small arrow cone mesh at tip position, pointing along direction. */
function arrowCone(from: THREE.Vector3, to: THREE.Vector3, color: number): THREE.Mesh {
  const dir = new THREE.Vector3().subVectors(to, from).normalize();
  // Place cone just before target (so it touches the node box)
  const tip = to.clone().addScaledVector(dir, -0.25);
  const coneGeo = new THREE.ConeGeometry(0.12, 0.35, 8);
  const coneMat = new THREE.MeshStandardMaterial({ color, emissive: color, emissiveIntensity: 0.4 });
  const cone = new THREE.Mesh(coneGeo, coneMat);
  cone.position.copy(tip);
  // Orient cone: default points +Y, we want +dir
  cone.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), dir);
  return cone;
}

export default function Preview3D({ state }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // Read the design tokens once (single token source — DESIGN §2).
    const palette = readPalette();

    // ── Scene setup ──────────────────────────────────────────────
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(palette.bg);
    scene.fog = new THREE.Fog(palette.bg, 20, 80);

    const w = container.clientWidth;
    const h = container.clientHeight;

    const camera = new THREE.PerspectiveCamera(50, w / h, 0.5, 200);
    camera.position.set(0, 18, 22);
    camera.lookAt(0, 0, 0);

    // WebGL renderer
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.shadowMap.enabled = true;
    container.appendChild(renderer.domElement);

    // CSS2D renderer (for edge labels)
    const labelRenderer = new CSS2DRenderer();
    labelRenderer.setSize(w, h);
    labelRenderer.domElement.style.position = 'absolute';
    labelRenderer.domElement.style.top = '0';
    labelRenderer.domElement.style.pointerEvents = 'none';
    container.appendChild(labelRenderer.domElement);

    // Orbit controls
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.target.set(0, 0, 0);
    controls.update();

    // ── Lighting ─────────────────────────────────────────────────
    const ambient = new THREE.AmbientLight(0x404060, 2.5);
    scene.add(ambient);
    const dirLight = new THREE.DirectionalLight(0xffffff, 3);
    dirLight.position.set(10, 20, 10);
    dirLight.castShadow = true;
    dirLight.shadow.mapSize.set(1024, 1024);
    scene.add(dirLight);
    const hemi = new THREE.HemisphereLight(0x6060c0, 0x202030, 1.5);
    scene.add(hemi);

    // ── Grid floor ───────────────────────────────────────────────
    const grid = new THREE.GridHelper(40, 40, palette.gridMajor, palette.gridMinor);
    scene.add(grid);

    // ── Node group ───────────────────────────────────────────────
    const nodesGroup = new THREE.Group();
    scene.add(nodesGroup);

    // Compute centroid for centering the graph
    let cx = 0, cy = 0;
    if (state.rooms.length > 0) {
      cx = state.rooms.reduce((s, r) => s + r.x, 0) / state.rooms.length;
      cy = state.rooms.reduce((s, r) => s + r.y, 0) / state.rooms.length;
    }
    const SCALE = 0.015; // map pixel coords → 3D units

    // Node lookup for edge building
    const nodePos3 = new Map<string, THREE.Vector3>();

    // ── Create nodes ─────────────────────────────────────────────
    for (const room of state.rooms) {
      const pos = new THREE.Vector3(
        (room.x - cx) * SCALE,
        0,
        (room.y - cy) * SCALE,
      );
      nodePos3.set(room.node_id, pos);

      const color = nodeColor(room, palette);
      const hasPois = room.pois.length > 0;

      // Box for rooms, cylinder for rooms with POIs
      let geo: THREE.BufferGeometry;
      if (hasPois) {
        geo = new THREE.CylinderGeometry(0.35, 0.4, 0.9, 16);
      } else {
        geo = new THREE.BoxGeometry(0.55, 0.55, 0.55);
      }
      const mat = new THREE.MeshStandardMaterial({
        color,
        roughness: 0.35,
        metalness: 0.3,
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.position.copy(pos);
      mesh.position.y = hasPois ? 0.45 : 0.275;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      nodesGroup.add(mesh);

      // Label above node
      const labelDiv = document.createElement('div');
      labelDiv.textContent = room.label;
      labelDiv.style.color = palette.text; // --wb-text
      labelDiv.style.fontSize = '10px';
      labelDiv.style.fontWeight = '600';
      labelDiv.style.fontFamily = 'system-ui, sans-serif';
      labelDiv.style.textShadow = '0 0 6px #000';
      labelDiv.style.whiteSpace = 'nowrap';
      const label = new CSS2DObject(labelDiv);
      label.position.copy(pos);
      label.position.y = hasPois ? 1.15 : 0.85;
      nodesGroup.add(label);
    }

    // ── Create edges ─────────────────────────────────────────────
    const edgesGroup = new THREE.Group();
    scene.add(edgesGroup);

    for (const edge of state.edges) {
      const from = nodePos3.get(edge.from_node);
      const to = nodePos3.get(edge.to_node);
      if (!from || !to) continue;

      const color = edge.bidirectional ? palette.edgeBi : palette.edgeUni;

      // Line segment (raised slightly above nodes)
      const mid = new THREE.Vector3().addVectors(from, to).multiplyScalar(0.5);
      // Arc the line slightly upward for visual clarity
      const points = [
        new THREE.Vector3(from.x, 0.05, from.z),
        new THREE.Vector3(mid.x, 0.35, mid.z),
        new THREE.Vector3(to.x, 0.05, to.z),
      ];
      const curve = new THREE.QuadraticBezierCurve3(points[0], points[1], points[2]);
      const curvePoints = curve.getPoints(24);
      const lineGeo = new THREE.BufferGeometry().setFromPoints(curvePoints);
      const lineMat = new THREE.LineBasicMaterial({ color, linewidth: 1 });
      const line = new THREE.Line(lineGeo, lineMat);
      edgesGroup.add(line);

      // Arrow cone at target end
      const coneTarget = arrowCone(from, to, color);
      edgesGroup.add(coneTarget);

      // Second arrow for bidirectional
      if (edge.bidirectional) {
        const coneSource = arrowCone(to, from, color);
        edgesGroup.add(coneSource);
      }

      // Edge label (CSS2D)
      if (edge.label) {
        const labelDiv = document.createElement('div');
        labelDiv.textContent = edge.label;
        // Label text: a lightened tint of the edge hue. The raw edge token is a
        // neutral stroke color — too low-contrast as 9px text on the dark scrim
        // (WCAG AA) — so lerp toward --wb-text (stays token-derived) for legibility.
        const labelHue = new THREE.Color(color).lerp(new THREE.Color(palette.text), 0.55).getHex();
        labelDiv.style.color = hexCss(labelHue);
        labelDiv.style.fontSize = '9px';
        labelDiv.style.fontWeight = '600';
        labelDiv.style.fontFamily = 'system-ui, sans-serif';
        labelDiv.style.background = palette.scrim; // --wb-bg-canvas @ 0.85
        labelDiv.style.padding = '1px 6px';
        labelDiv.style.borderRadius = '4px';
        labelDiv.style.whiteSpace = 'nowrap';
        const cssObj = new CSS2DObject(labelDiv);
        const midPt = curve.getPointAt(0.5);
        cssObj.position.copy(midPt);
        cssObj.position.y += 0.2;
        edgesGroup.add(cssObj);
      }
    }

    // ── Animation loop ───────────────────────────────────────────
    let animId: number;
    function animate() {
      animId = requestAnimationFrame(animate);
      controls.update();
      renderer.render(scene, camera);
      labelRenderer.render(scene, camera);
    }
    animate();

    // ── Resize ───────────────────────────────────────────────────
    const el = container;
    function onResize() {
      const cw = el.clientWidth;
      const ch = el.clientHeight;
      camera.aspect = cw / ch;
      camera.updateProjectionMatrix();
      renderer.setSize(cw, ch);
      labelRenderer.setSize(cw, ch);
    }
    window.addEventListener('resize', onResize);

    // ── Cleanup ──────────────────────────────────────────────────
    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener('resize', onResize);
      controls.dispose();
      renderer.dispose();
      container.removeChild(renderer.domElement);
      container.removeChild(labelRenderer.domElement);
    };
  }, [state]);

  return (
    <div
      ref={containerRef}
      style={{
        width: '100%',
        height: 'calc(100vh - 56px)',
        position: 'relative',
        overflow: 'hidden',
      }}
    />
  );
}
