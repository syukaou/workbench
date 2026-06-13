import { Handle, Position, type NodeProps } from 'reactflow';
import type { POI } from './types';

interface PoiNodeData {
  label: string;
  marks: string[];
  pois: POI[];
}

/** Derive a compact label from POI ids (e.g. "Boss", "Item×2"). */
function derivePoiLabel(pois: POI[]): string | null {
  if (pois.length === 0) return null;
  // Group by first token of poi_id (split on - or _)
  const groups: Record<string, number> = {};
  for (const poi of pois) {
    const token = poi.poi_id.split(/[-_]/)[0];
    if (token) groups[token] = (groups[token] || 0) + 1;
  }
  const entries = Object.entries(groups);
  if (entries.length === 0) return `POI×${pois.length}`;
  const fmt = (name: string, count: number) => {
    const cap = name.charAt(0).toUpperCase() + name.slice(1).toLowerCase();
    return count === 1 ? cap : `${cap}×${count}`;
  };
  if (entries.length === 1) {
    return fmt(entries[0][0], entries[0][1]);
  }
  // Up to 2 types shown
  return entries.slice(0, 2).map(([n, c]) => fmt(n, c)).join(', ');
}

export default function PoiNode({ data, selected }: NodeProps<PoiNodeData>) {
  const { label, marks, pois } = data;
  const badgeLabel = derivePoiLabel(pois);

  return (
    <div className={`poi-node ${selected ? 'selected' : ''}`}>
      <Handle type="target" position={Position.Top} />
      <div className="poi-node-label">{label}</div>
      {marks.length > 0 && (
        <div className="poi-node-marks">
          {marks.map((m) => (
            <span key={m} className="poi-node-mark">{m}</span>
          ))}
        </div>
      )}
      {badgeLabel && (
        <div className="poi-badge" title={`${pois.length} POI(s)`}>
          {badgeLabel}
        </div>
      )}
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}
