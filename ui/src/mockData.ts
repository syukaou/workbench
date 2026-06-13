/**
 * Core data layer — replaces mockData.ts with real WASM calls.
 *
 * The Rust core stores only topology (nodes, edges, marks, POIs).
 * UI positions (x, y) are managed locally in React state.
 */
import type { GraphState, RoomNode, EdgeDef } from './types';
import { ensureCore, getCoreState, executeCoreCommand } from './coreBridge';

// ── Local position store (not in core) ───────────────────────────────

/** Auto-assigned positions for nodes not yet placed. */
const positionCache = new Map<string, { x: number; y: number }>();
let nextAutoX = 300;
let nextAutoY = 200;

function autoPosition(nodeId: string): { x: number; y: number } {
  if (positionCache.has(nodeId)) {
    return positionCache.get(nodeId)!;
  }
  const pos = { x: nextAutoX, y: nextAutoY };
  nextAutoX += 180;
  if (nextAutoX > 700) {
    nextAutoX = 100;
    nextAutoY += 180;
  }
  positionCache.set(nodeId, pos);
  return pos;
}

export function getPosition(nodeId: string): { x: number; y: number } {
  return positionCache.get(nodeId) ?? autoPosition(nodeId);
}

export function setPosition(nodeId: string, x: number, y: number): void {
  positionCache.set(nodeId, { x, y });
}

// ── State transformation ─────────────────────────────────────────────

/**
 * Convert the core's flat HashMap state into GraphState.
 * Core keys: node:<id>, edge:<from>-><to>, entity_type:<name>, entity_instance:<id>
 */
export function coreStateToGraphState(coreState: Record<string, unknown>): GraphState {
  const rooms: RoomNode[] = [];
  const edges: EdgeDef[] = [];

  for (const [key, value] of Object.entries(coreState)) {
    if (key.startsWith('node:')) {
      const nodeId = key.slice(5);
      const data = value as Record<string, unknown>;
      const pos = getPosition(nodeId);
      rooms.push({
        node_id: nodeId,
        label: (data.label as string) ?? nodeId,
        x: pos.x,
        y: pos.y,
        marks: (data.marks as string[]) ?? [],
        pois: (data.pois as Array<{ poi_id: string; entity_ref: string | null }>) ?? [],
      });
    } else if (key.startsWith('edge:')) {
      const edgeKey = key.slice(5);
      const arrowIdx = edgeKey.indexOf('->');
      if (arrowIdx === -1) continue;
      const from_node = edgeKey.slice(0, arrowIdx);
      const to_node = edgeKey.slice(arrowIdx + 2);
      const data = value as Record<string, unknown>;
      edges.push({
        from_node,
        to_node,
        bidirectional: (data.bidirectional as boolean) ?? false,
      });
    }
  }

  return { rooms, edges };
}

// ── Public API ───────────────────────────────────────────────────────

/** Load initial state from the WASM core. */
export async function loadState(): Promise<GraphState> {
  await ensureCore();
  const coreState = getCoreState();
  return coreStateToGraphState(coreState);
}

/** Execute a topology command and return the updated GraphState. */
export async function executeCommand(cmdObj: Record<string, unknown>): Promise<GraphState> {
  const result = executeCoreCommand(cmdObj);
  if (!result.ok) {
    throw new Error(`Core command failed: ${result.error}`);
  }
  // Reload full state from core after mutation
  const coreState = getCoreState();
  return coreStateToGraphState(coreState);
}

// ── Fallback mock (for when WASM is unavailable) ─────────────────────

export function loadMockState(): GraphState {
  const rooms: RoomNode[] = [
    { node_id: 'entrance', label: 'Entrance Hall', x: 400, y: 300, marks: ['spawn'], pois: [{ poi_id: 'entrance-door', entity_ref: null }] },
    { node_id: 'armory', label: 'Armory', x: 150, y: 150, marks: [], pois: [{ poi_id: 'weapon-rack', entity_ref: null }] },
    { node_id: 'library', label: 'Library', x: 650, y: 150, marks: [], pois: [{ poi_id: 'scroll-table', entity_ref: null }] },
    { node_id: 'garden', label: 'Garden', x: 400, y: 80, marks: [], pois: [] },
    { node_id: 'vault', label: 'Vault', x: 150, y: 30, marks: ['shortcut'], pois: [{ poi_id: 'locked-chest', entity_ref: null }] },
  ];
  const edgeDefs: EdgeDef[] = [
    { from_node: 'entrance', to_node: 'armory', bidirectional: true },
    { from_node: 'entrance', to_node: 'library', bidirectional: true },
    { from_node: 'entrance', to_node: 'garden', bidirectional: true },
    { from_node: 'armory', to_node: 'vault', bidirectional: false },
    { from_node: 'library', to_node: 'garden', bidirectional: false },
  ];
  return { rooms, edges: edgeDefs };
}
