import { useState, useCallback } from 'react';
import type { GraphState } from './types';
import { Button } from './components/ui/button';
import { Separator } from './components/ui/separator';

interface Props {
  state: GraphState;
  onStateChange: (state: GraphState) => void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  mode: 'select' | 'add_edge';
  onSetMode: (m: 'select' | 'add_edge') => void;
  viewMode: '2d' | '3d';
  onToggleView: () => void;
  // AI Proposal
  proposals: Record<string, unknown>[] | null;
  proposalLoading: boolean;
  onPropose: (intent: string) => void;
  onAcceptAll: () => void;
  onRejectAll: () => void;
  onAcceptSingle: (cmd: Record<string, unknown>) => void;
  onRejectSingle: (cmd: Record<string, unknown>) => void;
  // v1.4: Save/Load
  onSave: () => void;
  onLoad: () => void;
  // Editable project name (shown in the top bar, used as the save filename).
  projectName: string;
  onProjectNameChange: (name: string) => void;
  coreReady: boolean;
}

let nodeCounter = 0;

/** Format a command object as a human-readable string. */
function describeCommand(cmd: Record<string, unknown>): string {
  const [variant, params] = Object.entries(cmd)[0] ?? ['?', {}];
  const p = params as Record<string, unknown>;
  switch (variant) {
    case 'CreateNode':
      return `+ Node "${p.node_id}" (${p.label})`;
    case 'RemoveNode':
      return `- Node "${p.node_id}"`;
    case 'CreateEdge':
      return `→ Edge ${p.from_node} → ${p.to_node}${p.bidirectional ? ' ↔' : ' (one-way)'}`;
    case 'RemoveEdge':
      return `✕ Edge ${p.from_node} → ${p.to_node}`;
    case 'MarkNode':
      return `🏷 Mark "${p.node_id}" as ${p.mark}`;
    default:
      return `${variant}: ${JSON.stringify(p)}`;
  }
}

export default function Toolbar({
  state, onStateChange, canUndo, canRedo, onUndo, onRedo, mode, onSetMode,
  viewMode, onToggleView,
  proposals, proposalLoading, onPropose, onAcceptAll, onRejectAll,
  onAcceptSingle, onRejectSingle, onSave, onLoad,
  projectName, onProjectNameChange, coreReady,
}: Props) {
  const [intentText, setIntentText] = useState('');
  const [showProposals, setShowProposals] = useState(false);

  const handleAddRoom = useCallback(() => {
    nodeCounter++;
    const nodeId = `room-${nodeCounter}`;
    const label = `Room ${nodeCounter}`;
    const newState: GraphState = {
      ...state,
      rooms: [
        ...state.rooms,
        {
          node_id: nodeId,
          label,
          x: 400 + Math.random() * 200 - 100,
          y: 300 + Math.random() * 200 - 100,
          marks: [],
          pois: [],
        },
      ],
    };
    onStateChange(newState);
  }, [state, onStateChange]);

  const handleAddEdge = useCallback(() => {
    onSetMode('add_edge');
  }, [onSetMode]);

  const handleProposeClick = useCallback(() => {
    const text = intentText.trim();
    if (!text) return;
    onPropose(text);
    setShowProposals(true);
    setIntentText('');
  }, [intentText, onPropose]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        handleProposeClick();
      }
    },
    [handleProposeClick],
  );

  return (
    <>
      <div className="toolbar">
        <h2 className="toolbar-title">Workbench Topology</h2>
        <input
          className="nodrag"
          value={projectName}
          placeholder="untitled"
          onChange={(e) => onProjectNameChange(e.target.value)}
          aria-label="Project name (used as the save filename)"
          title="Project name — used as the .workbench.json filename"
          style={{
            background: 'var(--wb-bg-inset)',
            border: '1px solid var(--wb-border)',
            borderRadius: 'var(--wb-radius-sm)',
            color: 'var(--wb-text)',
            font: 'inherit',
            fontSize: 'var(--wb-fs-sm)',
            padding: '3px 8px',
            width: '150px',
          }}
        />
        <div className="toolbar-buttons">
          <Button title="Add a new room" onClick={handleAddRoom}>
            ＋ Add Room
          </Button>
          <Button
            title="Add edge (click two nodes)"
            active={mode === 'add_edge'}
            onClick={handleAddEdge}
          >
            🔗 Add Edge
          </Button>
          <Separator orientation="vertical" />
          <Button title="Undo last action" onClick={onUndo} disabled={!canUndo}>
            ↶ Undo
          </Button>
          <Button title="Redo last undone action" onClick={onRedo} disabled={!canRedo}>
            ↷ Redo
          </Button>
          <Separator orientation="vertical" />
          <Button
            title="Save project (.workbench.json)"
            onClick={onSave}
            disabled={!coreReady}
          >
            💾 Save
          </Button>
          <Button
            title="Load project (.workbench.json)"
            onClick={onLoad}
            disabled={!coreReady}
          >
            📂 Load
          </Button>
          <Separator orientation="vertical" />
          <Button
            title={`Switch to ${viewMode === '2d' ? '3D preview' : '2D editor'}`}
            onClick={onToggleView}
          >
            {viewMode === '2d' ? '🌐 3D' : '📐 2D'}
          </Button>
          <Separator orientation="vertical" />
          <span className="toolbar-info">
            {state.rooms.length} rooms · {state.edges.length} edges
          </span>
        </div>
      </div>

      {/* ── AI Proposal Bar ──────────────────────────────────────────── */}
      <div className="proposal-bar">
        <div className="proposal-input-row">
          <input
            type="text"
            className="proposal-input"
            placeholder="e.g. 'a hub with 2 branches' or 'secret room'..."
            value={intentText}
            onChange={(e) => setIntentText(e.target.value)}
            onKeyDown={handleKeyDown}
          />
          <button
            className="proposal-btn"
            onClick={handleProposeClick}
            disabled={proposalLoading || intentText.trim().length === 0}
          >
            {proposalLoading ? '⏳' : '✨'} Propose
          </button>
        </div>
      </div>

      {/* ── Proposal Results Panel ──────────────────────────────────── */}
      {showProposals && proposals !== null && (
        <div className="proposal-panel">
          <div className="proposal-panel-header">
            <span>AI Proposals ({proposals.length} commands)</span>
            <div className="proposal-panel-actions">
              <button
                className="proposal-accept-all"
                onClick={() => { onAcceptAll(); setShowProposals(false); }}
                disabled={proposals.length === 0}
              >
                ✓ Accept All
              </button>
              <button
                className="proposal-reject-all"
                onClick={() => { onRejectAll(); setShowProposals(false); }}
              >
                ✕ Reject All
              </button>
            </div>
          </div>
          <div className="proposal-list">
            {proposals.length === 0 ? (
              <div className="proposal-empty">No proposals generated.</div>
            ) : (
              proposals.map((cmd, i) => (
                <div key={i} className="proposal-item">
                  <span className="proposal-desc">{describeCommand(cmd)}</span>
                  <div className="proposal-item-actions">
                    <button
                      className="proposal-accept"
                      title="Accept this command"
                      onClick={() => {
                        onAcceptSingle(cmd);
                        if (proposals.length <= 1) setShowProposals(false);
                      }}
                    >
                      ✓
                    </button>
                    <button
                      className="proposal-reject"
                      title="Reject this command"
                      onClick={() => {
                        onRejectSingle(cmd);
                        if (proposals.length <= 1) setShowProposals(false);
                      }}
                    >
                      ✕
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </>
  );
}
