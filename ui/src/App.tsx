import { useState, useCallback, useRef, useEffect } from 'react';
import TopologyGraph from './TopologyGraph';
import Toolbar from './Toolbar';
import PoiEditor from './PoiEditor';
import {
  loadState,
  executeCommand,
  loadMockState,
  setPosition,
  requestProposal,
  getEntityState,
  executeCoreCommand,
} from './mockData';
import type { GraphState, EntityState } from './types';
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

  // ── POI Editor state ──────────────────────────────────────────────
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [entityState, setEntityState] = useState<EntityState>({ types: [], instances: [] });

  // ── AI Proposal state ──────────────────────────────────────────────
  const [proposals, setProposals] = useState<Record<string, unknown>[] | null>(null);
  const [proposalLoading, setProposalLoading] = useState(false);

  // ── POI counter ───────────────────────────────────────────────────
  let poiCounterRef = useRef(0);

  // ── Init: load from WASM core (fallback to mock) ──────────────────

  useEffect(() => {
    let cancelled = false;
    async function init() {
      try {
        const graphState = await loadState();
        if (!cancelled) {
          setState(graphState);
          setCoreReady(true);
          // Load entity state
          try {
            const es = getEntityState();
            setEntityState(es);
          } catch { /* entity state unavailable */ }
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

  // ── Refresh all state from core ───────────────────────────────────

  const refreshState = useCallback(async () => {
    if (!coreReady) return;
    try {
      const freshState = await loadState();
      setState(freshState);
      const es = getEntityState();
      setEntityState(es);
    } catch { /* keep current state */ }
  }, [coreReady]);

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

      try {
        for (const room of newState.rooms) {
          const oldRoom = state.rooms.find((r) => r.node_id === room.node_id);
          if (!oldRoom) {
            await executeCommand({ CreateNode: { node_id: room.node_id, label: room.label } });
            setPosition(room.node_id, room.x, room.y);
          }
        }
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

  // ── Toggle edge ───────────────────────────────────────────────────

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

  // ── Node select (POI editor) ──────────────────────────────────────

  const handleNodeSelect = useCallback((nodeId: string | null) => {
    if (mode === 'add_edge') {
      // In add_edge mode, treat as edge endpoint selection
      if (nodeId) handleNodeClick(nodeId);
      return;
    }
    setSelectedNodeId(nodeId);
    // Refresh entity state
    if (coreReady && nodeId) {
      try {
        const es = getEntityState();
        setEntityState(es);
      } catch { /* ignore */ }
    }
  }, [mode, handleNodeClick, coreReady]);

  // ── POI operations ────────────────────────────────────────────────

  const handleAttachPoi = useCallback(
    (nodeId: string, poiId: string, entityRef: string | null) => {
      executeCoreCommand({
        AttachPOI: { node_id: nodeId, poi_id: poiId, entity_ref: entityRef ?? null },
      });
      refreshState();
    },
    [refreshState],
  );

  const handleDetachPoi = useCallback(
    (nodeId: string, poiId: string) => {
      executeCoreCommand({
        DetachPOI: { node_id: nodeId, poi_id: poiId },
      });
      refreshState();
    },
    [refreshState],
  );

  const handleSetField = useCallback(
    (instanceId: string, field: string, value: unknown) => {
      executeCoreCommand({
        SetEntityField: { instance_id: instanceId, field, value },
      });
      refreshState();
    },
    [refreshState],
  );

  const handleCreateInstanceAndAttach = useCallback(
    (nodeId: string, poiId: string, entityType: string, fields: Record<string, string>) => {
      poiCounterRef.current++;
      const instanceId = `inst-${poiCounterRef.current}-${poiId}`;

      // Create instance
      const r1 = executeCoreCommand({
        CreateEntityInstance: { entity_type: entityType, instance_id: instanceId },
      });
      if (!r1.ok) {
        console.warn('CreateEntityInstance failed:', r1.error);
        return;
      }

      // Set fields
      for (const [field, value] of Object.entries(fields)) {
        if (value) {
          executeCoreCommand({
            SetEntityField: { instance_id: instanceId, field, value },
          });
        }
      }

      // Attach POI
      const r2 = executeCoreCommand({
        AttachPOI: { node_id: nodeId, poi_id: poiId, entity_ref: instanceId },
      });
      if (!r2.ok) {
        console.warn('AttachPOI failed:', r2.error);
      }

      refreshState();
    },
    [refreshState],
  );

  // ── AI Proposal handlers ──────────────────────────────────────────

  const handlePropose = useCallback(async (intent: string) => {
    setProposalLoading(true);
    setProposals(null);
    try {
      const cmds = await requestProposal(intent);
      setProposals(cmds);
    } catch (err) {
      console.warn('Proposal failed:', err);
      setProposals([]);
    } finally {
      setProposalLoading(false);
    }
  }, []);

  const handleAcceptProposals = useCallback(async () => {
    if (!proposals || !state) return;
    const newCommands = [...proposals];
    setProposals(null);
    for (const cmd of newCommands) {
      try {
        const updatedState = await executeCommand(cmd);
        setState(updatedState);
      } catch (err) {
        console.warn('Command execution failed:', err);
      }
    }
    if (coreReady) {
      try {
        const freshState = await loadState();
        setState(freshState);
      } catch { /* keep current state */ }
    }
  }, [proposals, state, coreReady]);

  const handleRejectProposals = useCallback(() => {
    setProposals(null);
  }, []);

  const handleAcceptSingle = useCallback(async (cmd: Record<string, unknown>) => {
    if (!state) return;
    try {
      const updatedState = await executeCommand(cmd);
      setState(updatedState);
      setProposals((prev) => prev?.filter((c) => c !== cmd) ?? null);
    } catch (err) {
      console.warn('Command execution failed:', err);
    }
  }, [state]);

  const handleRejectSingle = useCallback((cmd: Record<string, unknown>) => {
    setProposals((prev) => prev?.filter((c) => c !== cmd) ?? null);
  }, []);

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

  // ── Selected node for POI editor ──────────────────────────────────

  const selectedNode = selectedNodeId
    ? state.rooms.find((r) => r.node_id === selectedNodeId) ?? null
    : null;

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
        proposals={proposals}
        proposalLoading={proposalLoading}
        onPropose={handlePropose}
        onAcceptAll={handleAcceptProposals}
        onRejectAll={handleRejectProposals}
        onAcceptSingle={handleAcceptSingle}
        onRejectSingle={handleRejectSingle}
      />
      <div className="main-area">
        <TopologyGraph
          state={state}
          onStateChange={handleStateChange}
          onToggleEdge={handleToggleEdge}
          onNodeSelect={handleNodeSelect}
        />
        {selectedNode && (
          <PoiEditor
            nodeId={selectedNode.node_id}
            nodeLabel={selectedNode.label}
            pois={selectedNode.pois}
            entityState={entityState}
            onAttachPoi={handleAttachPoi}
            onDetachPoi={handleDetachPoi}
            onSetField={handleSetField}
            onCreateInstanceAndAttach={handleCreateInstanceAndAttach}
            onClose={() => setSelectedNodeId(null)}
          />
        )}
      </div>
    </div>
  );
}
