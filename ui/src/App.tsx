import { useState, useCallback, useEffect, useMemo } from 'react';
import TopologyGraph from './TopologyGraph';
import { proposalsToOverlay, type Overlay } from './proposalOverlay';
import Preview3D from './Preview3D';
import Toolbar from './Toolbar';
import Sidebar from './Sidebar';
import EmptyState from './EmptyState';
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
  // The first-picked node in add_edge mode (the edge's source). State, not a
  // ref, so the canvas can highlight it and show a "click another node" hint.
  const [edgeSource, setEdgeSource] = useState<string | null>(null);

  // Mode switching clears any half-started edge: leaving add_edge must drop the
  // pending source so re-entering starts clean.
  const handleSetMode = useCallback((m: 'select' | 'add_edge') => {
    setMode(m);
    if (m !== 'add_edge') setEdgeSource(null);
  }, []);

  // ── POI Editor state ──────────────────────────────────────────────
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [entityState, setEntityState] = useState<EntityState>({ types: [], instances: [] });

  // ── AI Proposal state ──────────────────────────────────────────────
  const [proposals, setProposals] = useState<Record<string, unknown>[] | null>(null);
  const [proposalLoading, setProposalLoading] = useState(false);

  // ── Save/Load UX ────────────────────────────────────────────────────
  // Project name is UI-only metadata stored in the save wrapper, not core
  // state. `notice` is a transient, auto-dismissing banner for load/save
  // feedback (never a persisted truth).
  const [projectName, setProjectName] = useState('');
  const [notice, setNotice] = useState<{ kind: 'success' | 'error'; text: string } | null>(null);

  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(t);
  }, [notice]);

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
      } catch (err) {
        // WASM unavailable — fall back to mock data (view-only, no core truth).
        console.warn('WASM core unavailable, using mock data:', err);
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

  // ── Global keyboard flow ──────────────────────────────────────────
  // Ctrl/Cmd+Z undo, Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y redo (reusing the
  // core-routed handlers), Esc cancels a half-started edge. Never fire while a
  // text field is focused — typing must not trigger undo/redo (INV: writes only
  // through the explicit handlers, this just invokes them).
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const editable =
        !!target &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.isContentEditable);

      if (e.key === 'Escape' && !editable) {
        if (mode === 'add_edge') {
          handleSetMode('select');
        }
        return;
      }

      if (editable) return;
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      const key = e.key.toLowerCase();
      if (key === 'z' && !e.shiftKey) {
        e.preventDefault();
        handleUndo();
      } else if ((key === 'z' && e.shiftKey) || key === 'y') {
        e.preventDefault();
        handleRedo();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [mode, handleSetMode, handleUndo, handleRedo]);

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
        setEdgeSource(null);
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
      setEdgeSource(null);
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

  // ── Structural deletes / marks (core-routed, undoable) ────────────
  // Each is a single core command → one event → one Undo step.

  const handleRemoveNode = useCallback(
    async (nodeId: string) => {
      if (!coreReady) return;
      const r = executeCoreCommand({ RemoveNode: { node_id: nodeId } });
      if (!r.ok) {
        console.warn('RemoveNode failed:', r.error);
        return;
      }
      if (selectedNodeId === nodeId) setSelectedNodeId(null);
      await refreshState();
    },
    [coreReady, selectedNodeId, refreshState],
  );

  const handleRemoveEdge = useCallback(
    async (from: string, to: string) => {
      if (!coreReady) return;
      const r = executeCoreCommand({ RemoveEdge: { from_node: from, to_node: to } });
      if (!r.ok) {
        console.warn('RemoveEdge failed:', r.error);
        return;
      }
      await refreshState();
    },
    [coreReady, refreshState],
  );

  const handleMarkNode = useCallback(
    async (nodeId: string, mark: string) => {
      if (!coreReady) return;
      const r = executeCoreCommand({ MarkNode: { node_id: nodeId, mark } });
      if (!r.ok) {
        console.warn('MarkNode failed:', r.error);
        return;
      }
      await refreshState();
    },
    [coreReady, refreshState],
  );

  // ── Node click (add_edge mode) ────────────────────────────────────

  const handleNodeClick = useCallback(
    (nodeId: string) => {
      if (!state) return;
      if (mode !== 'add_edge') return;
      if (!edgeSource) {
        // First pick = the edge source; highlight it and prompt for the target.
        setEdgeSource(nodeId);
        return;
      }
      if (edgeSource === nodeId) return;
      const from = edgeSource;
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
      setEdgeSource(null);
      setMode('select');
    },
    [mode, state, edgeSource, coreReady, refreshState],
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

  // Re-bind a POI to an existing entity instance: detach then re-attach so the
  // node never accrues duplicate poi_id entries. Two core events, both undoable.
  const handleBindPoiEntity = useCallback(
    (nodeId: string, poiId: string, entityRef: string | null) => {
      executeCoreCommand({ DetachPOI: { node_id: nodeId, poi_id: poiId } });
      executeCoreCommand({ AttachPOI: { node_id: nodeId, poi_id: poiId, entity_ref: entityRef ?? null } });
      refreshState();
    },
    [refreshState],
  );

  const handleCreateEntityType = useCallback(
    (name: string) => {
      const r = executeCoreCommand({ CreateEntityType: { name } });
      if (!r.ok) {
        console.warn('CreateEntityType failed:', r.error);
        return;
      }
      refreshState();
    },
    [refreshState],
  );

  const handleCreateEntityInstance = useCallback(
    (entityType: string, instanceId: string) => {
      const r = executeCoreCommand({ CreateEntityInstance: { entity_type: entityType, instance_id: instanceId } });
      if (!r.ok) {
        console.warn('CreateEntityInstance failed:', r.error);
        return;
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
      // Determine the project name: reuse the current one, else ask (defaulting
      // to "untitled"). Cancelling the prompt aborts the save.
      let name = projectName.trim();
      if (!name) {
        const entered = window.prompt('Project name', 'untitled');
        if (entered === null) return;
        name = entered.trim() || 'untitled';
        setProjectName(name);
      }
      const json = buildProjectSave(name);
      // Derive the download filename from the name, keeping it filesystem-safe.
      const slug = name.replace(/[^\p{L}\p{N}._-]+/gu, '-').replace(/^-+|-+$/g, '') || 'untitled';
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${slug}.workbench.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      setNotice({ kind: 'success', text: `Saved “${name}”` });
    } catch (err) {
      console.warn('Save failed:', err);
      setNotice({ kind: 'error', text: 'Save failed — see console for details.' });
    }
  }, [projectName]);

  const handleLoad = useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json,.workbench.json';
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      try {
        const text = await file.text();
        // Rebuild goes through import_snapshot → event-log replay (red line).
        const { name } = await loadProject(text);
        // History cursor + state come straight back from the core.
        await refreshState();
        if (name) setProjectName(name);
        setNotice({ kind: 'success', text: `Loaded “${name ?? file.name}”` });
      } catch (err) {
        console.warn('Load failed:', err);
        setNotice({ kind: 'error', text: 'Load failed — file may be invalid.' });
      }
    };
    input.click();
  }, [refreshState]);

  // ── Empty-state quick actions ─────────────────────────────────────
  // Both route through existing core-backed paths; the empty card itself
  // writes nothing.

  const handleAddRoom = useCallback(() => {
    if (!state) return;
    const id = `room-${Date.now()}`;
    handleStateChange({
      ...state,
      rooms: [
        ...state.rooms,
        { node_id: id, label: `Room ${state.rooms.length + 1}`, x: 400, y: 300, marks: [], pois: [] },
      ],
    });
  }, [state, handleStateChange]);

  const handleFocusPropose = useCallback(() => {
    const el = document.querySelector('.proposal-input') as HTMLInputElement | null;
    el?.focus();
    el?.scrollIntoView({ block: 'nearest' });
  }, []);

  // ── AI proposal overlay (INV-3) — hook must run unconditionally ────
  // React requires hooks in the same order every render, so this useMemo lives
  // ABOVE the early loading return (moving it below caused "rendered more hooks
  // than during the previous render" once state loaded). Held proposals preview
  // on the canvas as pending (dashed/weakened) nodes/edges, derived at render
  // time; this NEVER writes core — accepting dispatches each command through the
  // core. The value is only consumed once state is non-null.
  const proposalOverlay = useMemo<Overlay>(
    () => (state ? proposalsToOverlay(proposals ?? [], state) : { rooms: [], edges: [] }),
    [proposals, state],
  );

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
        onSetMode={handleSetMode}
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
      {notice && (
        <div className={`app-notice app-notice--${notice.kind}`} role="status">
          {notice.text}
        </div>
      )}
      <div className="main-area">
        {viewMode === '2d' ? (
          <TopologyGraph
            state={state}
            onStateChange={handleStateChange}
            onToggleEdge={handleToggleEdge}
            onRemoveNode={handleRemoveNode}
            onRemoveEdge={handleRemoveEdge}
            onMarkNode={handleMarkNode}
            onNodeSelect={handleNodeSelect}
            overlay={proposalOverlay}
            mode={mode}
            edgeSource={edgeSource}
          />
        ) : (
          <Preview3D state={state} />
        )}
        {viewMode === '2d' && state.rooms.length === 0 && (
          <EmptyState onAddRoom={handleAddRoom} onFocusPropose={handleFocusPropose} />
        )}
        {viewMode === '2d' && (
          <Sidebar
            selectedNode={selectedNode}
            edges={state.edges}
            entityState={entityState}
            onAttachPoi={handleAttachPoi}
            onDetachPoi={handleDetachPoi}
            onBindPoiEntity={handleBindPoiEntity}
            onSetField={handleSetField}
            onCreateEntityType={handleCreateEntityType}
            onCreateEntityInstance={handleCreateEntityInstance}
            onRemoveNode={handleRemoveNode}
            onRemoveEdge={handleRemoveEdge}
            onDeselect={() => setSelectedNodeId(null)}
          />
        )}
      </div>
    </div>
  );
}
