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
  type NodeMouseHandler,
  applyNodeChanges,
  MarkerType,
  useNodesState,
  useEdgesState,
} from 'reactflow';
import 'reactflow/dist/style.css';
import type { GraphState, RoomNode } from './types';
import PoiNode from './PoiNode';

interface Props {
  state: GraphState;
  onStateChange: (state: GraphState) => void;
  onToggleEdge: (from: string, to: string) => void;
  onNodeSelect: (nodeId: string | null) => void;
}

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
    },
  }));
}

/** Convert U3 edges to React Flow edges. */
function edgesToRFEdges(edges: { from_node: string; to_node: string; bidirectional: boolean; label?: string }[]): Edge[] {
  return edges.map((e, i) => {
    const markerSize = { width: 20, height: 20 };
    const edge: Edge = {
      id: `${e.from_node}-${e.to_node}-${i}`,
      source: e.from_node,
      target: e.to_node,
      type: 'default',
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
    return edge;
  });
}

export default function TopologyGraph({ state, onStateChange, onToggleEdge, onNodeSelect }: Props) {
  const [nodes, setNodes, onNodesChangeRF] = useNodesState(roomsToNodes(state.rooms));
  const [edges, setEdges, onEdgesChangeRF] = useEdgesState(edgesToRFEdges(state.edges));

  // Sync external state changes back into React Flow internal state
  useMemo(() => {
    setNodes(roomsToNodes(state.rooms));
    setEdges(edgesToRFEdges(state.edges));
  }, [state, setNodes, setEdges]);

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

  const onPaneClick = useCallback(
    (_event: React.MouseEvent) => {
      // Placeholder: future click-to-add-node, handled by toolbar for now.
    },
    [],
  );

  const onEdgeContext = useCallback(
    (_event: React.MouseEvent, edge: Edge) => {
      _event.preventDefault();
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
      // Single click: select node for POI editing
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
        onPaneClick={onPaneClick}
        onEdgeContextMenu={onEdgeContext}
        onNodeClick={onNodeDoubleClick}
        fitView
        deleteKeyCode={null}
      >
        <Background color="var(--wb-border)" gap={20} />
        <Controls />
      </ReactFlow>
    </div>
  );
}
