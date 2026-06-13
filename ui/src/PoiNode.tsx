import { Handle, Position, type NodeProps } from 'reactflow';
import type { POI } from './types';

interface PoiNodeData {
  label: string;
  marks: string[];
  pois: POI[];
}

export default function PoiNode({ data, selected }: NodeProps<PoiNodeData>) {
  const { label, marks, pois } = data;
  const poiCount = pois.length;

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
      {poiCount > 0 && (
        <div className="poi-badge" title={`${poiCount} POI(s)`}>
          {poiCount}
        </div>
      )}
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
}
