import { useCallback, useMemo } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  type Node,
  type Edge,
  type Connection,
  type OnNodesChange,
  type OnEdgesChange,
  type OnNodesDelete,
  type OnEdgesDelete,
  type NodeMouseHandler,
  applyNodeChanges,
  MarkerType,
  useNodesState,
  useEdgesState,
} from 'reactflow';
import 'reactflow/dist/style.css';
import type { GraphState, RoomNode, EdgeDef } from './types';
import PoiNode from './PoiNode';
import { composeView, type Overlay } from './proposalOverlay';

interface Props {
  state: GraphState;
  onStateChange: (state: GraphState) => void;
  onToggleEdge: (from: string, to: string) => void;
  onRemoveNode: (nodeId: string) => void;
  onRemoveEdge: (from: string, to: string) => void;
  onMarkNode: (nodeId: string, mark: string) => void;
  onNodeSelect: (nodeId: string | null) => void;
  /** AI proposal preview (pending nodes/edges) — render-only, never committed (INV-3). */
  overlay?: Overlay;
}

const EMPTY_OVERLAY: Overlay = { rooms: [], edges: [] };

const nodeTypes = { poiNode: PoiNode };

/** Convert U3 rooms to React Flow nodes. */
function roomsToNodes(rooms: RoomNode[]): Node[] {
  return rooms.map((r) => ({
    id: r.node_id,
    type: 'poiNode',
    position: { x: r.x, y: r.y },
    data: {
      label: r.label,
      marks: r.marks,
      pois: r.pois,
      pending: r.pending ?? false,
    },
  }));
}

/**
 * Convert U3 edges to React Flow edges.
 *
 * Only edges whose endpoints both still exist are rendered: when a node is
 * deleted via core (RemoveNode), the core leaves its incident edges in place
 * (event-sourced — they reappear when the delete is undone), so we filter the
 * dangling ones out of the projection rather than rendering edges to nowhere.
 */
function edgesToRFEdges(edges: EdgeDef[], roomIds: Set<string>): Edge[] {
  return edges
    .filter((e) => roomIds.has(e.from_node) && roomIds.has(e.to_node))
    .map((e, i) => {
    const markerSize = { width: 20, height: 20 };
    const edge: Edge = {
      id: `${e.from_node}-${e.to_node}-${i}`,
      source: e.from_node,
      target: e.to_node,
      type: 'default',
      data: { pending: e.pending ?? false },
      markerEnd: { type: MarkerType.ArrowClosed, ...markerSize },
      // Canvas reads the single design-token source (DESIGN §2).
      style: { stroke: 'var(--wb-edge-bi)', strokeWidth: 2 },
    };
    if (e.bidirectional) {
      edge.markerStart = { type: MarkerType.ArrowClosed, ...markerSize };
      edge.style = { stroke: 'var(--wb-edge-bi)', strokeWidth: 3 };
    } else {
      // One-way edges: thicker, animated dash to indicate direction
      edge.style = { stroke: 'var(--wb-edge-uni)', strokeWidth: 3 };
      edge.animated = true;
    }
    if (e.pending) {
      // Pending = AI proposal not yet accepted (INV-3): dashed + weakened,
      // accent hue, no flow animation (it isn't real connectivity yet).
      edge.style = {
        stroke: 'var(--wb-edge-pending)',
        strokeWidth: 2,
        strokeDasharray: '6 4',
        opacity: 0.6,
      };
      edge.animated = false;
    }
    return edge;
  });
}

export default function TopologyGraph({
  state,
  onStateChange,
  onToggleEdge,
  onRemoveNode,
  onRemoveEdge,
  onMarkNode,
  onNodeSelect,
  overlay = EMPTY_OVERLAY,
}: Props) {
  // Render = committed projection + pending AI overlay (INV-3). The overlay is
  // composed at render time only; the editing handlers below still operate on
  // the committed `state` and skip pending items, so a preview never reaches core.
  const view = useMemo(() => composeView(state, overlay), [state, overlay]);
  const viewRoomIds = useMemo(() => new Set(view.rooms.map((r) => r.node_id)), [view]);
  const [nodes, setNodes, onNodesChangeRF] = useNodesState(roomsToNodes(view.rooms));
  const [edges, setEdges, onEdgesChangeRF] = useEdgesState(edgesToRFEdges(view.edges, viewRoomIds));

  // Sync external state changes back into React Flow internal state
  useMemo(() => {
    setNodes(roomsToNodes(view.rooms));
    setEdges(edgesToRFEdges(view.edges, viewRoomIds));
  }, [view, viewRoomIds, setNodes, setEdges]);

  const onConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target) return;
      const existing = state.edges.find(
        (e) => e.from_node === connection.source && e.to_node === connection.target,
      );
      if (existing) return;
      // Default new edge: bidirectional
      const newEdges = [...state.edges, { from_node: connection.source, to_node: connection.target, bidirectional: true }];
      onStateChange({ ...state, edges: newEdges });
    },
    [state, onStateChange],
  );

  const onNodesChange: OnNodesChange = useCallback(
    (changes) => {
      // Apply React Flow node changes (e.g. drag position) to internal state
      const result = applyNodeChanges(changes, nodes);
      // Persist positions back to our GraphState
      const updatedRooms = state.rooms.map((room) => {
        const updated = result.find((n) => n.id === room.node_id);
        if (updated) {
          return { ...room, x: updated.position.x, y: updated.position.y };
        }
        return room;
      });
      // Only update state when positions actually changed
      const moved = changes.some((c) => c.type === 'position' && c.dragging !== true);
      if (moved) {
        onStateChange({ ...state, rooms: updatedRooms });
      }
      onNodesChangeRF(changes);
    },
    [state, nodes, onNodesChangeRF, onStateChange],
  );

  const onEdgesChange: OnEdgesChange = useCallback(
    (changes) => onEdgesChangeRF(changes),
    [onEdgesChangeRF],
  );

  // Delete key → core RemoveNode. Incident edges cascade in React Flow's view
  // but stay in core (filtered out by edgesToRFEdges); a single Undo restores
  // the node and its edges together.
  const onNodesDelete: OnNodesDelete = useCallback(
    // Pending (proposal) nodes aren't in core — deleting them is just dropping
    // the preview, never a core RemoveNode.
    (deleted) => deleted.filter((n) => !n.data?.pending).forEach((n) => onRemoveNode(n.id)),
    [onRemoveNode],
  );

  // Delete key → core RemoveEdge, but only for edges the user explicitly
  // selected. Edges that React Flow cascades when a node is deleted come
  // through unselected — those must NOT hit core (the node delete owns them).
  const onEdgesDelete: OnEdgesDelete = useCallback(
    (deleted) =>
      deleted
        .filter((e) => e.selected && !e.data?.pending)
        .forEach((e) => onRemoveEdge(e.source, e.target)),
    [onRemoveEdge],
  );

  // Right-click a node → tag it with a semantic mark (spawn, shortcut, …).
  const onNodeContext: NodeMouseHandler = useCallback(
    (event, node) => {
      event.preventDefault();
      if (node.data?.pending) return; // can't mark a not-yet-accepted proposal node
      const mark = window.prompt(`Mark for "${node.id}" (e.g. spawn, shortcut):`)?.trim();
      if (mark) onMarkNode(node.id, mark);
    },
    [onMarkNode],
  );

  const onPaneClick = useCallback(
    (_event: React.MouseEvent) => {
      // Placeholder: future click-to-add-node, handled by toolbar for now.
    },
    [],
  );

  const onEdgeContext = useCallback(
    (_event: React.MouseEvent, edge: Edge) => {
      _event.preventDefault();
      if (edge.data?.pending) return; // proposal edge — not in core, nothing to toggle
      const fromNode = edge.source;
      const toNode = edge.target;
      // Find the edge in our state
      const match = state.edges.find(
        (e) => e.from_node === fromNode && e.to_node === toNode,
      );
      if (match) {
        onToggleEdge(match.from_node, match.to_node);
      }
    },
    [state.edges, onToggleEdge],
  );

  const onNodeDoubleClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      // Single click: select node for POI editing. Pending proposal nodes have
      // no core identity yet, so they aren't selectable for POI editing.
      if (node.data?.pending) return;
      onNodeSelect(node.id);
    },
    [onNodeSelect],
  );

  return (
    <div className="topology-graph" style={{ width: '100%', height: 'calc(100vh - 56px)' }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onNodesDelete={onNodesDelete}
        onEdgesDelete={onEdgesDelete}
        onPaneClick={onPaneClick}
        onEdgeContextMenu={onEdgeContext}
        onNodeContextMenu={onNodeContext}
        onNodeClick={onNodeDoubleClick}
        fitView
        deleteKeyCode={['Delete', 'Backspace']}
      >
        <Background color="var(--wb-border)" gap={20} />
        <Controls />
      </ReactFlow>
    </div>
  );
}
