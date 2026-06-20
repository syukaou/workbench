/**
 * Proposal overlay — the visual landing of INV-3 (AI is proposer, not decider).
 *
 * AI returns typed commands. BEFORE the human accepts, those commands must be
 * previewable on the canvas WITHOUT touching the core. We do that purely at
 * render time: interpret the proposed commands into temporary (pending) nodes /
 * edges, then compose them on top of the committed projection for React Flow.
 *
 * Nothing here writes the core and nothing here lives in React's source of
 * truth — the overlay is recomputed from the held proposal each render. On
 * accept, App dispatches each command through the core one-by-one (one event
 * each); on reject the proposal is simply dropped and this overlay disappears.
 */
import type { GraphState, RoomNode, EdgeDef } from './types';

export interface Overlay {
  rooms: RoomNode[];
  edges: EdgeDef[];
}

/** Read the first variant/params pair from a typed command object. */
function variantOf(cmd: Record<string, unknown>): [string, Record<string, unknown>] {
  const [variant, params] = Object.entries(cmd)[0] ?? ['', {}];
  return [variant, (params ?? {}) as Record<string, unknown>];
}

/**
 * Interpret proposed commands into pending nodes / edges (view shapes only,
 * same RoomNode / EdgeDef the committed projection uses — U1R reuse).
 *
 * Pending nodes get auto-layout coordinates in a fresh row laid out *below*
 * the committed content's bounding box, so a preview never lands on top of
 * existing rooms. Layout is a pure function of (committed, commands) → stable
 * across renders, so pending cards don't jitter.
 *
 * Only topology commands are visualized (CreateNode / CreateEdge / MarkNode);
 * other variants are carried by the typed-diff list, not the canvas.
 */
export function proposalsToOverlay(
  commands: Record<string, unknown>[],
  committed: GraphState,
): Overlay {
  const committedIds = new Set(committed.rooms.map((r) => r.node_id));

  // Lay pending nodes out in a row just below the committed content.
  const maxY = committed.rooms.reduce((m, r) => Math.max(m, r.y), 0);
  const baseY = committed.rooms.length > 0 ? maxY + 200 : 200;
  const startX = 120;
  const stepX = 180;

  const rooms: RoomNode[] = [];
  const roomById = new Map<string, RoomNode>();
  const edges: EdgeDef[] = [];
  let col = 0;

  for (const cmd of commands) {
    const [variant, p] = variantOf(cmd);
    switch (variant) {
      case 'CreateNode': {
        const id = String(p.node_id ?? '');
        if (!id || committedIds.has(id) || roomById.has(id)) break;
        const room: RoomNode = {
          node_id: id,
          label: String(p.label ?? id),
          x: startX + col * stepX,
          y: baseY,
          marks: [],
          pois: [],
          pending: true,
        };
        col++;
        rooms.push(room);
        roomById.set(id, room);
        break;
      }
      case 'MarkNode': {
        // A mark on a freshly-proposed node decorates its pending card.
        const id = String(p.node_id ?? '');
        const room = roomById.get(id);
        if (room && p.mark != null) room.marks = [...room.marks, String(p.mark)];
        break;
      }
      case 'CreateEdge': {
        const from_node = String(p.from_node ?? '');
        const to_node = String(p.to_node ?? '');
        if (!from_node || !to_node) break;
        edges.push({
          from_node,
          to_node,
          bidirectional: Boolean(p.bidirectional),
          pending: true,
        });
        break;
      }
      default:
        break;
    }
  }

  return { rooms, edges };
}

/**
 * Compose the committed projection with the pending overlay into one view for
 * React Flow. Committed items stay solid; overlay items carry `pending: true`
 * and render weakened/dashed. Overlay nodes whose id collides with a committed
 * node are dropped — the committed (solid) node wins.
 */
export function composeView(committed: GraphState, overlay: Overlay): GraphState {
  const committedIds = new Set(committed.rooms.map((r) => r.node_id));
  const overlayRooms = overlay.rooms.filter((r) => !committedIds.has(r.node_id));
  return {
    rooms: [...committed.rooms, ...overlayRooms],
    edges: [...committed.edges, ...overlay.edges],
  };
}
