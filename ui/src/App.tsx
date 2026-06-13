import { useState, useCallback, useRef, useEffect } from 'react';
import TopologyGraph from './TopologyGraph';
import Toolbar from './Toolbar';
import { loadState, executeCommand, loadMockState, setPosition } from './mockData';
import type { GraphState } from './types';
import './App.css';

export default function App() {
  const [state, setState] = useState<GraphState | null>(null);
  const [coreReady, setCoreReady] = useState(false);
  const [mode, setMode] = useState<'select' | 'add_edge'>('select');
  const undoStackRef = useRef<GraphState[]>([]);
  const redoStackRef = useRef<GraphState[]>([]);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const edgeSourceRef = useRef<string | null>(null);

  // ── Init: load from WASM core (fallback to mock) ──────────────────

  useEffect(() => {
    let cancelled = false;
    async function init() {
      try {
        const graphState = await loadState();
        if (!cancelled) {
          setState(graphState);
          setCoreReady(true);
        }
      } catch {
        // WASM unavailable — fall back to mock data
        console.warn('WASM core unavailable, using mock data');
        if (!cancelled) {
          setState(loadMockState());
        }
      }
    }
    init();
    return () => { cancelled = true; };
  }, []);

  // ── Undo/redo ─────────────────────────────────────────────────────

  const pushUndo = useCallback((prev: GraphState) => {
    undoStackRef.current.push(prev);
    redoStackRef.current = [];
    setCanUndo(true);
    setCanRedo(false);
  }, []);

  const handleUndo = useCallback(() => {
    const stack = undoStackRef.current;
    if (stack.length === 0 || !state) return;
    const prev = stack.pop()!;
    redoStackRef.current.push(state);
    setState(prev);
    setCanUndo(stack.length > 0);
    setCanRedo(true);
  }, [state]);

  const handleRedo = useCallback(() => {
    const stack = redoStackRef.current;
    if (stack.length === 0 || !state) return;
    const next = stack.pop()!;
    undoStackRef.current.push(state);
    setState(next);
    setCanRedo(stack.length > 0);
    setCanUndo(true);
  }, [state]);

  // ── State change — sends topology commands to WASM core ───────────

  const handleStateChange = useCallback(
    async (newState: GraphState) => {
      if (!state) return;
      pushUndo(state);
      setState(newState);

      // If core is ready, sync topology changes to WASM
      if (!coreReady) {
        setMode('select');
        edgeSourceRef.current = null;
        return;
      }

      // Detect what changed and send corresponding command
      try {
        // New rooms
        for (const room of newState.rooms) {
          const oldRoom = state.rooms.find((r) => r.node_id === room.node_id);
          if (!oldRoom) {
            // New room: send CreateNode + update position cache
            await executeCommand({ CreateNode: { node_id: room.node_id, label: room.label } });
            setPosition(room.node_id, room.x, room.y);
          }
        }
        // New edges
        for (const edge of newState.edges) {
          const oldEdge = state.edges.find(
            (e) => e.from_node === edge.from_node && e.to_node === edge.to_node,
          );
          if (!oldEdge) {
            await executeCommand({
              CreateEdge: {
                from_node: edge.from_node,
                to_node: edge.to_node,
                bidirectional: edge.bidirectional,
              },
            });
          }
        }
      } catch (err) {
        console.warn('Core sync failed:', err);
      }

      setMode('select');
      edgeSourceRef.current = null;
    },
    [state, pushUndo, coreReady],
  );

  // ── Toggle edge (UI-only for now) ─────────────────────────────────

  const handleToggleEdge = useCallback(
    (from: string, to: string) => {
      if (!state) return;
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

  // ── Node click (add_edge mode) ────────────────────────────────────

  const handleNodeClick = useCallback(
    (nodeId: string) => {
      if (!state) return;
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
            const newEdge = { from_node: from, to_node: to, bidirectional: true };
            pushUndo(state);
            setState({
              ...state,
              edges: [...state.edges, newEdge],
            });
            // Sync to core
            if (coreReady) {
              executeCommand({
                CreateEdge: { from_node: from, to_node: to, bidirectional: true },
              }).catch((err) => console.warn('Core sync failed:', err));
            }
          }
          edgeSourceRef.current = null;
          setMode('select');
        }
      }
    },
    [mode, state, pushUndo, coreReady],
  );

  // ── Loading state ─────────────────────────────────────────────────

  if (!state) {
    return (
      <div className="app">
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', height: '100vh', color: '#94a3b8' }}>
          Loading core...
        </div>
      </div>
    );
  }

  // ── Render ────────────────────────────────────────────────────────

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
