import { Button } from './components/ui/button';

interface Props {
  /** Add a room manually (same path as the toolbar's Add Room). */
  onAddRoom: () => void;
  /** Jump to the bottom AI proposal input so the user can generate a skeleton. */
  onFocusPropose: () => void;
}

/**
 * Centered guidance card shown when the canvas has no nodes (get_state has no
 * `node:` keys). Render-only — it owns no state and writes nothing; both actions
 * route through the same core-backed handlers the toolbar uses.
 */
export default function EmptyState({ onAddRoom, onFocusPropose }: Props) {
  return (
    <div className="empty-state">
      <div className="empty-state-card">
        <div className="empty-state-icon">🗺️</div>
        <h3 className="empty-state-title">Empty canvas</h3>
        <p className="empty-state-body">
          Start a level by generating a topology skeleton with the AI proposal bar
          below, or add a room by hand.
        </p>
        <div className="empty-state-actions">
          <Button variant="primary" onClick={onFocusPropose}>
            ✨ AI propose
          </Button>
          <Button onClick={onAddRoom}>＋ Add Room</Button>
        </div>
      </div>
    </div>
  );
}
