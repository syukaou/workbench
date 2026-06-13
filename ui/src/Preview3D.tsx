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

/** Pick node color based on marks / POI presence. */
function nodeColor(room: RoomNode): number {
  const marks = room.marks.map((m) => m.toLowerCase());
  const hasPois = room.pois.length > 0;

  // Boss / enemy rooms → red-ish
  if (marks.some((m) => m.includes('boss') || m.includes('enemy'))) return 0xef4444;
  // Treasure / loot rooms → gold
  if (marks.some((m) => m.includes('treasure') || m.includes('loot') || m.includes('item'))) return 0xf59e0b;
  // Start / spawn rooms → green
  if (marks.some((m) => m.includes('start') || m.includes('spawn') || m.includes('entrance'))) return 0x22c55e;
  // Exit / goal rooms → cyan
  if (marks.some((m) => m.includes('exit') || m.includes('goal') || m.includes('end'))) return 0x06b6d4;
  // Occupied rooms (has POIs) → teal
  if (hasPois) return 0x14b8a6;
  // Default → slate
  return 0x64748b;
}

/** Build a small arrow cone mesh at tip position, pointing along direction. */
function arrowCone(from: THREE.Vector3, to: THREE.Vector3, color: number): THREE.Mesh {
  const dir = new THREE.Vector3().subVectors(to, from).normalize();
  const len = from.distanceTo(to);
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

    // ── Scene setup ──────────────────────────────────────────────
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0f172a);
    scene.fog = new THREE.Fog(0x0f172a, 20, 80);

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
    const grid = new THREE.GridHelper(40, 40, 0x334155, 0x1e293b);
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

      const color = nodeColor(room);
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
      labelDiv.style.color = '#e2e8f0';
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

      const color = edge.bidirectional ? 0x3b82f6 : 0xf59e0b;

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
        labelDiv.style.color = color === 0x3b82f6 ? '#93c5fd' : '#fcd34d';
        labelDiv.style.fontSize = '9px';
        labelDiv.style.fontWeight = '600';
        labelDiv.style.fontFamily = 'system-ui, sans-serif';
        labelDiv.style.background = 'rgba(15, 23, 42, 0.85)';
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
    function onResize() {
      const cw = container.clientWidth;
      const ch = container.clientHeight;
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
