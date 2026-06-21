/**
 * Core data layer — replaces mockData.ts with real WASM calls.
 *
 * The Rust core stores only topology (nodes, edges, marks, POIs).
 * UI positions (x, y) are managed locally in React state.
 */
import type { GraphState, RoomNode, EdgeDef, EntityState, EntityTypeInfo, EntityInstanceInfo } from './types';
import { ensureCore, getCoreState, executeCoreCommand, proposeViaCore, exportCoreSnapshot, importCoreSnapshot, coreUndo, coreRedo, coreUndoRedoStatus, isCoreReady } from './coreBridge';

// Re-export executeCoreCommand for direct use by App
export { executeCoreCommand };
// Re-export core-as-truth undo/redo (event-log cursor) for App.
export { coreUndo, coreRedo, coreUndoRedoStatus };
// Re-export the core-readiness probe so callers can no-op before init.
export { isCoreReady };

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

// ── Entity state extraction ────────────────────────────────────────────

/** Extract entity types from core state. */
export function extractEntityTypes(coreState: Record<string, unknown>): EntityTypeInfo[] {
  const types: EntityTypeInfo[] = [];
  for (const [key, value] of Object.entries(coreState)) {
    if (key.startsWith('entity_type:')) {
      const name = key.slice('entity_type:'.length);
      const data = value as Record<string, unknown>;
      types.push({
        name,
        fields: (data.fields as Record<string, unknown>) ?? {},
      });
    }
  }
  return types;
}

/** Extract entity instances from core state. */
export function extractEntityInstances(coreState: Record<string, unknown>): EntityInstanceInfo[] {
  const instances: EntityInstanceInfo[] = [];
  for (const [key, value] of Object.entries(coreState)) {
    if (key.startsWith('entity_instance:')) {
      const instance_id = key.slice('entity_instance:'.length);
      const data = value as Record<string, unknown>;
      instances.push({
        instance_id,
        type: (data.type as string) ?? 'unknown',
        fields: (data.fields as Record<string, unknown>) ?? {},
      });
    }
  }
  return instances;
}

/** Get the current entity state from core. */
export function getEntityState(): EntityState {
  const coreState = getCoreState();
  return {
    types: extractEntityTypes(coreState),
    instances: extractEntityInstances(coreState),
  };
}

/** Load initial entity state from WASM. */
export async function loadEntityState(): Promise<EntityState> {
  await ensureCore();
  return getEntityState();
}

// ── Entity command helpers ─────────────────────────────────────────────

/** Create an entity type. */
export function createEntityType(name: string): { ok: boolean; error?: string } {
  return executeCoreCommand({ CreateEntityType: { name } });
}

/** Create an entity instance. */
export function createEntityInstance(entity_type: string, instance_id: string): { ok: boolean; error?: string } {
  return executeCoreCommand({ CreateEntityInstance: { entity_type, instance_id } });
}

/** Set a field value on an entity instance. */
export function setEntityField(instance_id: string, field: string, value: unknown): { ok: boolean; error?: string } {
  return executeCoreCommand({ SetEntityField: { instance_id, field, value } });
}

// ── AI Proposal ────────────────────────────────────────────────────────

const PROPOSE_URL = 'http://localhost:5198/propose';

/** Where a proposal came from: the real AI HTTP server, or a local keyword mock. */
export type ProposalSource = 'http' | 'mock';

export interface ProposalResult {
  commands: Record<string, unknown>[];
  source: ProposalSource;
}

/**
 * Request topology proposals via HTTP (real AI via CLI server).
 * Falls back to WASM mock, then to a local mock if everything fails.
 * The returned `source` lets the UI tell the user when it's running on a mock
 * (the AI server isn't up) — this is transient UI metadata, never core truth.
 */
export async function requestProposal(intent: string): Promise<ProposalResult> {
  // 1) Try the native HTTP proposal server (real AI via opencode CLI).
  try {
    const cmds = await proposeViaHttp(intent);
    if (cmds.length > 0) return { commands: cmds, source: 'http' };
  } catch {
    // Not an error: the UI already surfaces this as the "using local mock" hint
    // (P3), so keep it as an info-level breadcrumb rather than a warning.
    console.info('HTTP proposal server unavailable, trying WASM mock...');
  }

  // 2) Fall back to WASM mock (keyword-based proposal generator).
  try {
    await ensureCore();
    return { commands: proposeViaCore(intent), source: 'mock' };
  } catch {
    console.warn('WASM core unavailable for proposal, using local mock');
  }

  // 3) Last resort: local JavaScript mock (same keyword logic).
  return { commands: mockProposeLocal(intent), source: 'mock' };
}

/**
 * Call the native HTTP proposal server: POST localhost:5198/propose.
 * Returns parsed commands array, or throws on failure.
 */
async function proposeViaHttp(intent: string): Promise<Record<string, unknown>[]> {
  const response = await fetch(PROPOSE_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ intent }),
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`Propose server returned ${response.status}: ${body}`);
  }

  const data = await response.json();
  if (!data.ok) {
    throw new Error(`Propose server error: ${data.error}`);
  }

  return (data.commands ?? []) as Record<string, unknown>[];
}

/** Local mock proposal generator matching the Rust mock keyword logic. */
function mockProposeLocal(intent: string): Record<string, unknown>[] {
  const lower = intent.toLowerCase();
  const cmds: Record<string, unknown>[] = [];

  if (lower.includes('branch') || lower.includes('fork') || lower.includes('hub')) {
    cmds.push({ CreateNode: { node_id: 'hub', label: 'Central Hub' } });
    cmds.push({ CreateNode: { node_id: 'branch_a', label: 'Branch A' } });
    cmds.push({ CreateNode: { node_id: 'branch_b', label: 'Branch B' } });
    cmds.push({ CreateEdge: { from_node: 'hub', to_node: 'branch_a', bidirectional: true } });
    cmds.push({ CreateEdge: { from_node: 'hub', to_node: 'branch_b', bidirectional: true } });
    cmds.push({ MarkNode: { node_id: 'hub', mark: 'spawn' } });
  } else if (lower.includes('loop') || lower.includes('circle') || lower.includes('cycle')) {
    cmds.push({ CreateNode: { node_id: 'loop_a', label: 'Loop A' } });
    cmds.push({ CreateNode: { node_id: 'loop_b', label: 'Loop B' } });
    cmds.push({ CreateNode: { node_id: 'loop_c', label: 'Loop C' } });
    cmds.push({ CreateEdge: { from_node: 'loop_a', to_node: 'loop_b', bidirectional: true } });
    cmds.push({ CreateEdge: { from_node: 'loop_b', to_node: 'loop_c', bidirectional: true } });
    cmds.push({ CreateEdge: { from_node: 'loop_c', to_node: 'loop_a', bidirectional: true } });
    cmds.push({ MarkNode: { node_id: 'loop_a', mark: 'spawn' } });
  } else if (lower.includes('shortcut') || lower.includes('skip')) {
    cmds.push({ CreateNode: { node_id: 'start', label: 'Start' } });
    cmds.push({ CreateNode: { node_id: 'mid', label: 'Midway' } });
    cmds.push({ CreateNode: { node_id: 'end', label: 'End' } });
    cmds.push({ CreateEdge: { from_node: 'start', to_node: 'mid', bidirectional: true } });
    cmds.push({ CreateEdge: { from_node: 'mid', to_node: 'end', bidirectional: true } });
    cmds.push({ CreateEdge: { from_node: 'start', to_node: 'end', bidirectional: false } });
    cmds.push({ MarkNode: { node_id: 'start', mark: 'spawn' } });
    cmds.push({ MarkNode: { node_id: 'end', mark: 'shortcut' } });
  } else {
    cmds.push({ CreateNode: { node_id: 'room_1', label: 'Room 1' } });
    cmds.push({ CreateNode: { node_id: 'room_2', label: 'Room 2' } });
    cmds.push({ CreateNode: { node_id: 'room_3', label: 'Room 3' } });
    cmds.push({ CreateEdge: { from_node: 'room_1', to_node: 'room_2', bidirectional: true } });
    cmds.push({ CreateEdge: { from_node: 'room_2', to_node: 'room_3', bidirectional: true } });
    cmds.push({ MarkNode: { node_id: 'room_1', mark: 'spawn' } });
  }
  return cmds;
}

// ── v1.4: Save/Load persistence ──────────────────────────────────────

/** Export all cached positions as a plain object. */
export function exportPositions(): Record<string, { x: number; y: number }> {
  const obj: Record<string, { x: number; y: number }> = {};
  positionCache.forEach((pos, nodeId) => {
    obj[nodeId] = { ...pos };
  });
  return obj;
}

/** Import positions from a plain object into the cache. */
export function importPositions(data: Record<string, { x: number; y: number }>): void {
  positionCache.clear();
  for (const [nodeId, pos] of Object.entries(data)) {
    positionCache.set(nodeId, { x: pos.x, y: pos.y });
  }
  // Reset auto-position counters so new nodes don't overlap imported ones
  nextAutoX = 300;
  nextAutoY = 200;
}

/**
 * Build a full project save file: core snapshot + UI positions + project name.
 * `name` is UI-only metadata (not core design state) so it lives in the save
 * wrapper alongside the UI-only positions, never inside the event log.
 */
export function buildProjectSave(name: string): string {
  const coreSnapshot = JSON.parse(exportCoreSnapshot());
  const positions = exportPositions();
  const project = {
    name,
    ...coreSnapshot,
    positions,
    savedAt: new Date().toISOString(),
  };
  return JSON.stringify(project, null, 2);
}

/**
 * Restore a project from a save file. Returns the rebuilt GraphState plus the
 * saved project name (null if the file predates named saves). Rebuild always
 * goes through import_snapshot → event-log replay (red line: no second truth).
 */
export async function loadProject(jsonStr: string): Promise<{ state: GraphState; name: string | null }> {
  const project = JSON.parse(jsonStr);

  // Restore positions first
  if (project.positions && typeof project.positions === 'object') {
    importPositions(project.positions as Record<string, { x: number; y: number }>);
  }

  // Strip extra fields before passing to core (core only needs version + events + state)
  const corePayload = JSON.stringify({
    version: project.version,
    events: project.events,
    state: project.state,
  });

  const result = importCoreSnapshot(corePayload);
  if (!result.ok) {
    throw new Error(`Import failed: ${result.error}`);
  }

  // Re-read state from core
  const coreState = getCoreState();
  const name = typeof project.name === 'string' && project.name.trim() ? project.name : null;
  return { state: coreStateToGraphState(coreState), name };
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
