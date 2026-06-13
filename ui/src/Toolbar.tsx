import { useCallback } from 'react';
import type { GraphState } from './types';

interface Props {
  state: GraphState;
  onStateChange: (state: GraphState) => void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  mode: 'select' | 'add_edge';
  onSetMode: (m: 'select' | 'add_edge') => void;
}

let nodeCounter = 0;

export default function Toolbar({ state, onStateChange, canUndo, canRedo, onUndo, onRedo, mode, onSetMode }: Props) {

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

  return (
    <div className="toolbar">
      <h2 className="toolbar-title">Workbench Topology</h2>
      <div className="toolbar-buttons">
        <button title="Add a new room" onClick={handleAddRoom}>
          ＋ Add Room
        </button>
        <button
          title="Add edge (click two nodes)"
          className={mode === 'add_edge' ? 'active' : ''}
          onClick={handleAddEdge}
        >
          🔗 Add Edge
        </button>
        <span className="toolbar-separator" />
        <button title="Undo last action" onClick={onUndo} disabled={!canUndo}>
          ↶ Undo
        </button>
        <button title="Redo last undone action" onClick={onRedo} disabled={!canRedo}>
          ↷ Redo
        </button>
        <span className="toolbar-separator" />
        <span className="toolbar-info">
          {state.rooms.length} rooms · {state.edges.length} edges
        </span>
      </div>
    </div>
  );
}
