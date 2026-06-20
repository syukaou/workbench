import { useState, useCallback } from 'react';
import type { RoomNode, EdgeDef, EntityState, EntityInstanceInfo } from './types';

interface Props {
  /** Currently selected node, or null when nothing is selected. */
  selectedNode: RoomNode | null;
  /** All committed edges (used to list a node's incident edges for deletion). */
  edges: EdgeDef[];
  entityState: EntityState;
  onAttachPoi: (nodeId: string, poiId: string, entityRef: string | null) => void;
  onDetachPoi: (nodeId: string, poiId: string) => void;
  /** Re-bind a POI to an existing entity instance (detach + attach in core). */
  onBindPoiEntity: (nodeId: string, poiId: string, entityRef: string | null) => void;
  onSetField: (instanceId: string, field: string, value: unknown) => void;
  onCreateEntityType: (name: string) => void;
  onCreateEntityInstance: (entityType: string, instanceId: string) => void;
  onRemoveNode: (nodeId: string) => void;
  onRemoveEdge: (from: string, to: string) => void;
  /** Deselect the current node. */
  onDeselect: () => void;
}

/** Find an entity instance by ref id. */
function findInstance(instances: EntityInstanceInfo[], refId: string | null | undefined): EntityInstanceInfo | undefined {
  if (!refId) return undefined;
  return instances.find((i) => i.instance_id === refId);
}

/**
 * Right context sidebar (U3R). Every write goes through a core command handler
 * and the parent re-reads get_state() — React holds no authoritative model
 * state (INV-1/INV-2). The panel has two areas:
 *   1. Selected node — POI list / attach / detach / bind entity / delete.
 *   2. Entities — define types, create instances, edit fields, browse.
 */
export default function Sidebar({
  selectedNode,
  edges,
  entityState,
  onAttachPoi,
  onDetachPoi,
  onBindPoiEntity,
  onSetField,
  onCreateEntityType,
  onCreateEntityInstance,
  onRemoveNode,
  onRemoveEdge,
  onDeselect,
}: Props) {
  return (
    <div className="poi-editor">
      <div className="poi-editor-body">
        {selectedNode ? (
          <NodePanel
            node={selectedNode}
            edges={edges}
            entityState={entityState}
            onAttachPoi={onAttachPoi}
            onDetachPoi={onDetachPoi}
            onBindPoiEntity={onBindPoiEntity}
            onSetField={onSetField}
            onRemoveNode={onRemoveNode}
            onRemoveEdge={onRemoveEdge}
            onDeselect={onDeselect}
          />
        ) : (
          <div className="poi-editor-empty">
            Select a node to edit its POIs, or manage entities below.
          </div>
        )}

        <div className="sb-section-divider" />

        <EntityManager
          entityState={entityState}
          onCreateEntityType={onCreateEntityType}
          onCreateEntityInstance={onCreateEntityInstance}
          onSetField={onSetField}
        />
      </div>
    </div>
  );
}

// ── Selected-node panel: POIs + incident edges + delete ───────────────

interface NodePanelProps {
  node: RoomNode;
  edges: EdgeDef[];
  entityState: EntityState;
  onAttachPoi: (nodeId: string, poiId: string, entityRef: string | null) => void;
  onDetachPoi: (nodeId: string, poiId: string) => void;
  onBindPoiEntity: (nodeId: string, poiId: string, entityRef: string | null) => void;
  onSetField: (instanceId: string, field: string, value: unknown) => void;
  onRemoveNode: (nodeId: string) => void;
  onRemoveEdge: (from: string, to: string) => void;
  onDeselect: () => void;
}

function NodePanel({
  node,
  edges,
  entityState,
  onAttachPoi,
  onDetachPoi,
  onBindPoiEntity,
  onSetField,
  onRemoveNode,
  onRemoveEdge,
  onDeselect,
}: NodePanelProps) {
  const [newPoiId, setNewPoiId] = useState('');
  const [newPoiRef, setNewPoiRef] = useState('');

  const incidentEdges = edges.filter(
    (e) => e.from_node === node.node_id || e.to_node === node.node_id,
  );

  const handleAddPoi = useCallback(() => {
    const id = newPoiId.trim();
    if (!id) return;
    onAttachPoi(node.node_id, id, newPoiRef || null);
    setNewPoiId('');
    setNewPoiRef('');
  }, [node.node_id, newPoiId, newPoiRef, onAttachPoi]);

  return (
    <div className="sb-block">
      <div className="poi-editor-header">
        <h3 className="poi-editor-title">
          Node: <span className="poi-editor-node-name">{node.label}</span>
        </h3>
        <button className="poi-editor-close" onClick={onDeselect} title="Deselect">
          ✕
        </button>
      </div>

      {/* ── POI list ──────────────────────────────────────────────── */}
      {node.pois.length === 0 && <div className="poi-editor-empty">No POIs on this node.</div>}

      {node.pois.map((poi) => {
        const instance = findInstance(entityState.instances, poi.entity_ref);
        return (
          <div key={poi.poi_id} className="poi-item">
            <div className="poi-item-header">
              <span className="poi-item-id">{poi.poi_id}</span>
              {instance && <span className="poi-item-type">{instance.type}</span>}
              <button
                className="poi-item-delete"
                title="Detach POI"
                onClick={() => onDetachPoi(node.node_id, poi.poi_id)}
              >
                🗑
              </button>
            </div>

            {/* Bind to an existing entity instance (detach + attach in core). */}
            <div className="poi-field-row">
              <label className="poi-field-label">entity</label>
              <select
                className="poi-field-input"
                value={poi.entity_ref ?? ''}
                onChange={(e) => onBindPoiEntity(node.node_id, poi.poi_id, e.target.value || null)}
              >
                <option value="">— none —</option>
                {entityState.instances.map((inst) => (
                  <option key={inst.instance_id} value={inst.instance_id}>
                    {inst.instance_id} ({inst.type})
                  </option>
                ))}
              </select>
            </div>

            {/* Edit the bound instance's fields inline. */}
            {instance && (
              <div className="poi-item-fields">
                {Object.entries(instance.fields).map(([field, value]) => (
                  <div key={field} className="poi-field-row">
                    <label className="poi-field-label">{field}</label>
                    <input
                      className="poi-field-input"
                      type="text"
                      value={String(value ?? '')}
                      onChange={(e) => onSetField(instance.instance_id, field, e.target.value)}
                    />
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}

      {/* ── Add POI ───────────────────────────────────────────────── */}
      <div className="poi-add-form">
        <div className="poi-add-row">
          <label className="poi-add-label">New POI ID</label>
          <input
            className="poi-add-input"
            type="text"
            placeholder="e.g. boss-spawn"
            value={newPoiId}
            onChange={(e) => setNewPoiId(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleAddPoi()}
          />
        </div>
        <div className="poi-add-row">
          <label className="poi-add-label">Bind entity (optional)</label>
          <select
            className="poi-add-select"
            value={newPoiRef}
            onChange={(e) => setNewPoiRef(e.target.value)}
          >
            <option value="">— none —</option>
            {entityState.instances.map((inst) => (
              <option key={inst.instance_id} value={inst.instance_id}>
                {inst.instance_id} ({inst.type})
              </option>
            ))}
          </select>
        </div>
        <div className="poi-add-actions">
          <button className="poi-add-btn" onClick={handleAddPoi} disabled={!newPoiId.trim()}>
            ＋ Attach POI
          </button>
        </div>
      </div>

      {/* ── Incident edges (delete) ───────────────────────────────── */}
      {incidentEdges.length > 0 && (
        <div className="sb-edges">
          <div className="poi-add-label">Edges</div>
          {incidentEdges.map((e) => (
            <div key={`${e.from_node}->${e.to_node}`} className="sb-edge-row">
              <span className="sb-edge-label">
                {e.from_node} {e.bidirectional ? '↔' : '→'} {e.to_node}
              </span>
              <button
                className="poi-item-delete"
                title="Remove edge"
                onClick={() => onRemoveEdge(e.from_node, e.to_node)}
              >
                🗑
              </button>
            </div>
          ))}
        </div>
      )}

      {/* ── Delete node ───────────────────────────────────────────── */}
      <button className="sb-delete-node" onClick={() => onRemoveNode(node.node_id)}>
        Delete node
      </button>
    </div>
  );
}

// ── Entity management: types, instances, fields ───────────────────────

interface EntityManagerProps {
  entityState: EntityState;
  onCreateEntityType: (name: string) => void;
  onCreateEntityInstance: (entityType: string, instanceId: string) => void;
  onSetField: (instanceId: string, field: string, value: unknown) => void;
}

function EntityManager({
  entityState,
  onCreateEntityType,
  onCreateEntityInstance,
  onSetField,
}: EntityManagerProps) {
  const [newTypeName, setNewTypeName] = useState('');
  const [newInstType, setNewInstType] = useState('');
  const [newInstId, setNewInstId] = useState('');

  const handleCreateType = useCallback(() => {
    const name = newTypeName.trim();
    if (!name) return;
    onCreateEntityType(name);
    setNewTypeName('');
  }, [newTypeName, onCreateEntityType]);

  const handleCreateInstance = useCallback(() => {
    const type = newInstType || entityState.types[0]?.name;
    const id = newInstId.trim();
    if (!type || !id) return;
    onCreateEntityInstance(type, id);
    setNewInstId('');
  }, [newInstType, newInstId, entityState.types, onCreateEntityInstance]);

  return (
    <div className="sb-block">
      <h3 className="poi-editor-title sb-block-title">Entities</h3>

      {/* ── Define an entity type ─────────────────────────────────── */}
      <div className="poi-add-form">
        <div className="poi-add-row">
          <label className="poi-add-label">Define type</label>
          <input
            className="poi-add-input"
            type="text"
            placeholder="e.g. Boss"
            value={newTypeName}
            onChange={(e) => setNewTypeName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleCreateType()}
          />
        </div>
        <div className="poi-add-actions">
          <button className="poi-add-btn" onClick={handleCreateType} disabled={!newTypeName.trim()}>
            ＋ Type
          </button>
        </div>
      </div>

      {/* ── Create an instance ────────────────────────────────────── */}
      {entityState.types.length > 0 && (
        <div className="poi-add-form">
          <div className="poi-add-row">
            <label className="poi-add-label">New instance</label>
            <select
              className="poi-add-select"
              value={newInstType || entityState.types[0]?.name || ''}
              onChange={(e) => setNewInstType(e.target.value)}
            >
              {entityState.types.map((t) => (
                <option key={t.name} value={t.name}>
                  {t.name}
                </option>
              ))}
            </select>
          </div>
          <div className="poi-add-row">
            <input
              className="poi-add-input"
              type="text"
              placeholder="instance id, e.g. boss-1"
              value={newInstId}
              onChange={(e) => setNewInstId(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleCreateInstance()}
            />
          </div>
          <div className="poi-add-actions">
            <button className="poi-add-btn" onClick={handleCreateInstance} disabled={!newInstId.trim()}>
              ＋ Instance
            </button>
          </div>
        </div>
      )}

      {/* ── Browse types ──────────────────────────────────────────── */}
      {entityState.types.length > 0 && (
        <div className="sb-list">
          <div className="poi-add-label">Types ({entityState.types.length})</div>
          {entityState.types.map((t) => (
            <div key={t.name} className="sb-list-row">
              <span className="poi-item-type">{t.name}</span>
            </div>
          ))}
        </div>
      )}

      {/* ── Browse / edit instances ───────────────────────────────── */}
      {entityState.instances.length === 0 ? (
        <div className="poi-editor-empty">No entity instances yet.</div>
      ) : (
        <div className="sb-list">
          <div className="poi-add-label">Instances ({entityState.instances.length})</div>
          {entityState.instances.map((inst) => (
            <InstanceCard key={inst.instance_id} instance={inst} onSetField={onSetField} />
          ))}
        </div>
      )}
    </div>
  );
}

// ── A single instance card with editable + addable fields ─────────────

function InstanceCard({
  instance,
  onSetField,
}: {
  instance: EntityInstanceInfo;
  onSetField: (instanceId: string, field: string, value: unknown) => void;
}) {
  return (
    <div className="poi-item">
      <div className="poi-item-header">
        <span className="poi-item-id">{instance.instance_id}</span>
        <span className="poi-item-type">{instance.type}</span>
      </div>
      <div className="poi-item-fields">
        {Object.entries(instance.fields).map(([field, value]) => (
          <div key={field} className="poi-field-row">
            <label className="poi-field-label">{field}</label>
            <input
              className="poi-field-input"
              type="text"
              value={String(value ?? '')}
              onChange={(e) => onSetField(instance.instance_id, field, e.target.value)}
            />
          </div>
        ))}
        <div className="poi-field-row">
          <label className="poi-field-label">+ field</label>
          <input
            className="poi-field-input poi-field-new"
            placeholder="name, Enter to add"
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const input = e.currentTarget;
                const fieldName = input.value.trim();
                if (fieldName) {
                  onSetField(instance.instance_id, fieldName, '');
                  input.value = '';
                }
              }
            }}
          />
        </div>
      </div>
    </div>
  );
}
