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

interface Props {
  state: GraphState;
  onStateChange: (state: GraphState) => void;
  onToggleEdge: (from: string, to: string) => void;
  onNodeClick: (nodeId: string) => void;
}

/** Convert U3 rooms to React Flow nodes. */
function roomsToNodes(rooms: RoomNode[]): Node[] {
  return rooms.map((r) => ({
    id: r.node_id,
    type: 'default',
    position: { x: r.x, y: r.y },
    data: {
      label: r.label,
      marks: r.marks,
      pois: r.pois,
    },
  }));
}

/** Convert U3 edges to React Flow edges. */
function edgesToRFEdges(edges: { from_node: string; to_node: string; bidirectional: boolean }[]): Edge[] {
  return edges.map((e, i) => {
    const edge: Edge = {
      id: `${e.from_node}-${e.to_node}-${i}`,
      source: e.from_node,
      target: e.to_node,
      type: 'default',
      markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
      style: { stroke: '#64748b', strokeWidth: 2 },
    };
    if (e.bidirectional) {
      edge.markerStart = { type: MarkerType.ArrowClosed, width: 16, height: 16 };
      edge.style = { ...edge.style, stroke: '#3b82f6', strokeWidth: 2.5 };
    }
    return edge;
  });
}

export default function TopologyGraph({ state, onStateChange, onToggleEdge, onNodeClick }: Props) {
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
      onNodeClick(node.id);
    },
    [onNodeClick],
  );

  return (
    <div className="topology-graph" style={{ width: '100%', height: 'calc(100vh - 56px)' }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onPaneClick={onPaneClick}
        onEdgeContextMenu={onEdgeContext}
        onNodeDoubleClick={onNodeDoubleClick}
        fitView
        deleteKeyCode={null}
      >
        <Background color="#e2e8f0" gap={20} />
        <Controls />
      </ReactFlow>
    </div>
  );
}
