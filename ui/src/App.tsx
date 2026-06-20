import { useState, useCallback, useRef, useEffect } from 'react';
import TopologyGraph from './TopologyGraph';
import Preview3D from './Preview3D';
import Toolbar from './Toolbar';
import PoiEditor from './PoiEditor';
import {
  loadState,
  loadMockState,
  setPosition,
  requestProposal,
  getEntityState,
  executeCoreCommand,
  buildProjectSave,
  loadProject,
  coreUndo,
  coreRedo,
  coreUndoRedoStatus,
} from './mockData';
import type { GraphState, EntityState } from './types';
import './App.css';

/**
 * core-as-truth (INV-1): the Rust core (via WASM) is the single source of truth
 * for all model state and history. React holds NO authoritative model state and
 * NO undo stack — `state` here is a render cache derived from the core, and
 * undo/redo walk the core's event-log cursor. The only UI-owned data is node
 * positions (view-only, never part of the event log).
 */
export default function App() {
  const [state, setState] = useState<GraphState | null>(null);
  const [coreReady, setCoreReady] = useState(false);
  const [mode, setMode] = useState<'select' | 'add_edge'>('select');
  const [viewMode, setViewMode] = useState<'2d' | '3d'>('2d');
  // Derived from the core's undo/redo cursor — never from a local stack.
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
  const poiCounterRef = useRef(0);

  // ── Refresh the render cache + history cursor from the core ────────
  // This is the ONLY way state becomes visible: read it back from the core
  // after every mutation. canUndo/canRedo come straight from the cursor.

  const refreshState = useCallback(async () => {
    if (!coreReady) return;
    try {
      const freshState = await loadState();
      setState(freshState);
      setEntityState(getEntityState());
      const { current_seq, total_events } = coreUndoRedoStatus();
      setCanUndo(current_seq > 0);
      setCanRedo(current_seq < total_events);
    } catch {
      /* keep current cache */
    }
  }, [coreReady]);

  // ── Init: load from WASM core (fallback to mock) ──────────────────

  useEffect(() => {
    let cancelled = false;
    async function init() {
      try {
        const graphState = await loadState();
        if (cancelled) return;
        setState(graphState);
        setCoreReady(true);
        try {
          setEntityState(getEntityState());
        } catch {
          /* entity state unavailable */
        }
        try {
          const { current_seq, total_events } = coreUndoRedoStatus();
          setCanUndo(current_seq > 0);
          setCanRedo(current_seq < total_events);
        } catch {
          /* status unavailable */
        }
      } catch {
        // WASM unavailable — fall back to mock data (view-only, no core truth).
        console.warn('WASM core unavailable, using mock data');
        if (!cancelled) setState(loadMockState());
      }
    }
    init();
    return () => {
      cancelled = true;
    };
  }, []);

  // ── Undo/redo — delegate to the core event log (INV-1/INV-5) ──────

  const handleUndo = useCallback(async () => {
    if (!coreReady) return;
    const r = coreUndo(1);
    if (!r.ok) {
      console.warn('Undo failed:', r.error);
      return;
    }
    await refreshState();
  }, [coreReady, refreshState]);

  const handleRedo = useCallback(async () => {
    if (!coreReady) return;
    const r = coreRedo(1);
    if (!r.ok) {
      console.warn('Redo failed:', r.error);
      return;
    }
    await refreshState();
  }, [coreReady, refreshState]);

  // ── Structural / position changes ─────────────────────────────────
  // Structural diffs (new nodes/edges) go through the core; node positions
  // are view-only and stay in the position cache (not the event log).

  const handleStateChange = useCallback(
    async (newState: GraphState) => {
      if (!state) return;

      // Persist UI-only positions (never part of the core model).
      for (const room of newState.rooms) {
        setPosition(room.node_id, room.x, room.y);
      }

      if (!coreReady) {
        setState(newState);
        setMode('select');
        edgeSourceRef.current = null;
        return;
      }

      let structural = false;
      try {
        for (const room of newState.rooms) {
          if (!state.rooms.find((r) => r.node_id === room.node_id)) {
            const res = executeCoreCommand({ CreateNode: { node_id: room.node_id, label: room.label } });
            if (res.ok) {
              setPosition(room.node_id, room.x, room.y);
              structural = true;
            }
          }
        }
        for (const edge of newState.edges) {
          if (!state.edges.find((e) => e.from_node === edge.from_node && e.to_node === edge.to_node)) {
            const res = executeCoreCommand({
              CreateEdge: {
                from_node: edge.from_node,
                to_node: edge.to_node,
                bidirectional: edge.bidirectional,
              },
            });
            if (res.ok) structural = true;
          }
        }
      } catch (err) {
        console.warn('Core sync failed:', err);
      }

      if (structural) {
        // Structural change landed in the event log — re-read the truth.
        await refreshState();
      } else {
        // Position-only change — keep the optimistic view cache.
        setState(newState);
      }

      setMode('select');
      edgeSourceRef.current = null;
    },
    [state, coreReady, refreshState],
  );

  // ── Toggle edge direction (core-true: remove + recreate flipped) ──

  const handleToggleEdge = useCallback(
    async (from: string, to: string) => {
      if (!state) return;
      const edge = state.edges.find((e) => e.from_node === from && e.to_node === to);
      if (!edge) return;

      if (!coreReady) {
        setState({
          ...state,
          edges: state.edges.map((e) =>
            e.from_node === from && e.to_node === to ? { ...e, bidirectional: !e.bidirectional } : e,
          ),
        });
        return;
      }

      const r1 = executeCoreCommand({ RemoveEdge: { from_node: from, to_node: to } });
      if (!r1.ok) {
        console.warn('RemoveEdge failed:', r1.error);
        return;
      }
      const r2 = executeCoreCommand({
        CreateEdge: { from_node: from, to_node: to, bidirectional: !edge.bidirectional },
      });
      if (!r2.ok) console.warn('CreateEdge failed:', r2.error);
      await refreshState();
    },
    [state, coreReady, refreshState],
  );

  // ── Node click (add_edge mode) ────────────────────────────────────

  const handleNodeClick = useCallback(
    (nodeId: string) => {
      if (!state) return;
      if (mode !== 'add_edge') return;
      if (!edgeSourceRef.current) {
        edgeSourceRef.current = nodeId;
        return;
      }
      if (edgeSourceRef.current === nodeId) return;
      const from = edgeSourceRef.current;
      const to = nodeId;
      const exists = state.edges.some((e) => e.from_node === from && e.to_node === to);
      if (!exists) {
        if (coreReady) {
          const r = executeCoreCommand({ CreateEdge: { from_node: from, to_node: to, bidirectional: true } });
          if (r.ok) {
            refreshState();
          } else {
            console.warn('CreateEdge failed:', r.error);
          }
        } else {
          setState({ ...state, edges: [...state.edges, { from_node: from, to_node: to, bidirectional: true }] });
        }
      }
      edgeSourceRef.current = null;
      setMode('select');
    },
    [mode, state, coreReady, refreshState],
  );

  // ── Node select (POI editor) ──────────────────────────────────────

  const handleNodeSelect = useCallback(
    (nodeId: string | null) => {
      if (mode === 'add_edge') {
        if (nodeId) handleNodeClick(nodeId);
        return;
      }
      setSelectedNodeId(nodeId);
      if (coreReady && nodeId) {
        try {
          setEntityState(getEntityState());
        } catch {
          /* ignore */
        }
      }
    },
    [mode, handleNodeClick, coreReady],
  );

  // ── POI operations (already core-routed; refresh re-reads truth) ──

  const handleAttachPoi = useCallback(
    (nodeId: string, poiId: string, entityRef: string | null) => {
      executeCoreCommand({ AttachPOI: { node_id: nodeId, poi_id: poiId, entity_ref: entityRef ?? null } });
      refreshState();
    },
    [refreshState],
  );

  const handleDetachPoi = useCallback(
    (nodeId: string, poiId: string) => {
      executeCoreCommand({ DetachPOI: { node_id: nodeId, poi_id: poiId } });
      refreshState();
    },
    [refreshState],
  );

  const handleSetField = useCallback(
    (instanceId: string, field: string, value: unknown) => {
      executeCoreCommand({ SetEntityField: { instance_id: instanceId, field, value } });
      refreshState();
    },
    [refreshState],
  );

  const handleCreateInstanceAndAttach = useCallback(
    (nodeId: string, poiId: string, entityType: string, fields: Record<string, string>) => {
      poiCounterRef.current++;
      const instanceId = `inst-${poiCounterRef.current}-${poiId}`;

      const r1 = executeCoreCommand({ CreateEntityInstance: { entity_type: entityType, instance_id: instanceId } });
      if (!r1.ok) {
        console.warn('CreateEntityInstance failed:', r1.error);
        return;
      }
      for (const [field, value] of Object.entries(fields)) {
        if (value) {
          executeCoreCommand({ SetEntityField: { instance_id: instanceId, field, value } });
        }
      }
      const r2 = executeCoreCommand({ AttachPOI: { node_id: nodeId, poi_id: poiId, entity_ref: instanceId } });
      if (!r2.ok) console.warn('AttachPOI failed:', r2.error);

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
    if (!proposals) return;
    const cmds = [...proposals];
    setProposals(null);
    for (const cmd of cmds) {
      const r = executeCoreCommand(cmd);
      if (!r.ok) console.warn('Command execution failed:', r.error);
    }
    await refreshState();
  }, [proposals, refreshState]);

  const handleRejectProposals = useCallback(() => {
    setProposals(null);
  }, []);

  const handleAcceptSingle = useCallback(
    async (cmd: Record<string, unknown>) => {
      const r = executeCoreCommand(cmd);
      if (!r.ok) {
        console.warn('Command execution failed:', r.error);
        return;
      }
      setProposals((prev) => prev?.filter((c) => c !== cmd) ?? null);
      await refreshState();
    },
    [refreshState],
  );

  const handleRejectSingle = useCallback((cmd: Record<string, unknown>) => {
    setProposals((prev) => prev?.filter((c) => c !== cmd) ?? null);
  }, []);

  // ── Save/Load ─────────────────────────────────────────────────────

  const handleSave = useCallback(() => {
    try {
      const json = buildProjectSave();
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'project.workbench.json';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (err) {
      console.warn('Save failed:', err);
    }
  }, []);

  const handleLoad = useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json,.workbench.json';
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      try {
        const text = await file.text();
        await loadProject(text);
        // History cursor + state come straight back from the core.
        await refreshState();
      } catch (err) {
        console.warn('Load failed:', err);
      }
    };
    input.click();
  }, [refreshState]);

  // ── Loading state ─────────────────────────────────────────────────

  if (!state) {
    return (
      <div className="app">
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            height: '100vh',
            color: 'var(--wb-text-secondary)',
          }}
        >
          Loading core...
        </div>
      </div>
    );
  }

  // ── Selected node for POI editor ──────────────────────────────────

  const selectedNode = selectedNodeId ? state.rooms.find((r) => r.node_id === selectedNodeId) ?? null : null;

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
        viewMode={viewMode}
        onToggleView={() => setViewMode((v) => (v === '2d' ? '3d' : '2d'))}
        canUndo={canUndo}
        canRedo={canRedo}
        proposals={proposals}
        proposalLoading={proposalLoading}
        onPropose={handlePropose}
        onAcceptAll={handleAcceptProposals}
        onRejectAll={handleRejectProposals}
        onAcceptSingle={handleAcceptSingle}
        onRejectSingle={handleRejectSingle}
        onSave={handleSave}
        onLoad={handleLoad}
        coreReady={coreReady}
      />
      <div className="main-area">
        {viewMode === '2d' ? (
          <TopologyGraph
            state={state}
            onStateChange={handleStateChange}
            onToggleEdge={handleToggleEdge}
            onNodeSelect={handleNodeSelect}
          />
        ) : (
          <Preview3D state={state} />
        )}
        {viewMode === '2d' && selectedNode && (
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
