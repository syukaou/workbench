import { useState, useCallback, useRef } from 'react';
import TopologyGraph from './TopologyGraph';
import Toolbar from './Toolbar';
import { loadMockState } from './mockData';
import type { GraphState } from './types';
import './App.css';

export default function App() {
  const [state, setState] = useState<GraphState>(loadMockState);
  const [mode, setMode] = useState<'select' | 'add_edge'>('select');
  const undoStackRef = useRef<GraphState[]>([]);
  const redoStackRef = useRef<GraphState[]>([]);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  // For add_edge mode: remember first selected node
  const edgeSourceRef = useRef<string | null>(null);

  const pushUndo = useCallback((prev: GraphState) => {
    undoStackRef.current.push(prev);
    redoStackRef.current = [];
    setCanUndo(true);
    setCanRedo(false);
  }, []);

  const handleStateChange = useCallback(
    (newState: GraphState) => {
      pushUndo(state);
      setState(newState);
      // Reset edge mode after any state change
      setMode('select');
      edgeSourceRef.current = null;
    },
    [state, pushUndo],
  );

  const handleUndo = useCallback(() => {
    const stack = undoStackRef.current;
    if (stack.length === 0) return;
    const prev = stack.pop()!;
    redoStackRef.current.push(state);
    setState(prev);
    setCanUndo(stack.length > 0);
    setCanRedo(true);
  }, [state]);

  const handleRedo = useCallback(() => {
    const stack = redoStackRef.current;
    if (stack.length === 0) return;
    const next = stack.pop()!;
    undoStackRef.current.push(state);
    setState(next);
    setCanRedo(stack.length > 0);
    setCanUndo(true);
  }, [state]);

  const handleToggleEdge = useCallback(
    (from: string, to: string) => {
      const edges = state.edges.map((e) => {
        if (e.from_node === from && e.to_node === to) {
          return { ...e, bidirectional: !e.bidirectional };
        }
        return e;
      });
      const found = state.edges.some((e) => e.from_node === from && e.to_node === to);
      if (found) {
        pushUndo(state);
        setState({ ...state, edges });
      }
    },
    [state, pushUndo],
  );

  const handleNodeClick = useCallback(
    (nodeId: string) => {
      if (mode === 'add_edge') {
        if (!edgeSourceRef.current) {
          edgeSourceRef.current = nodeId;
        } else if (edgeSourceRef.current !== nodeId) {
          const from = edgeSourceRef.current;
          const to = nodeId;
          const exists = state.edges.some(
            (e) => e.from_node === from && e.to_node === to,
          );
          if (!exists) {
            pushUndo(state);
            setState({
              ...state,
              edges: [...state.edges, { from_node: from, to_node: to, bidirectional: true }],
            });
          }
          edgeSourceRef.current = null;
          setMode('select');
        }
      }
    },
    [mode, state, pushUndo],
  );

  return (
    <div className="app">
      <Toolbar
        state={state}
        onStateChange={handleStateChange}
        onUndo={handleUndo}
        onRedo={handleRedo}
        mode={mode}
        onSetMode={setMode}
        canUndo={canUndo}
        canRedo={canRedo}
      />
      <TopologyGraph
        state={state}
        onStateChange={handleStateChange}
        onToggleEdge={handleToggleEdge}
        onNodeClick={handleNodeClick}
      />
    </div>
  );
}
