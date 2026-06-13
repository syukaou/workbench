import { useState, useCallback } from 'react';
import type { POI, EntityState, EntityInstanceInfo } from './types';

interface Props {
  nodeId: string;
  nodeLabel: string;
  pois: POI[];
  entityState: EntityState;
  onAttachPoi: (nodeId: string, poiId: string, entityRef: string | null) => void;
  onDetachPoi: (nodeId: string, poiId: string) => void;
  onSetField: (instanceId: string, field: string, value: unknown) => void;
  onCreateInstanceAndAttach: (nodeId: string, poiId: string, entityType: string, fields: Record<string, string>) => void;
  onClose: () => void;
}

/** Get the entity instance for a given entity_ref. */
function findInstance(instances: EntityInstanceInfo[], refId: string | null): EntityInstanceInfo | undefined {
  if (!refId) return undefined;
  return instances.find((i) => i.instance_id === refId);
}

export default function PoiEditor({
  nodeId,
  nodeLabel,
  pois,
  entityState,
  onAttachPoi,
  onDetachPoi,
  onSetField,
  onCreateInstanceAndAttach,
  onClose,
}: Props) {
  const [showAddForm, setShowAddForm] = useState(false);
  const [newPoiId, setNewPoiId] = useState('');
  const [newPoiType, setNewPoiType] = useState(entityState.types[0]?.name ?? '');
  const [newPoiFields, setNewPoiFields] = useState<Record<string, string>>({});

  const handleAdd = useCallback(() => {
    if (!newPoiId.trim() || !newPoiType) return;
    onCreateInstanceAndAttach(nodeId, newPoiId.trim(), newPoiType, newPoiFields);
    setNewPoiId('');
    setNewPoiFields({});
    setShowAddForm(false);
  }, [nodeId, newPoiId, newPoiType, newPoiFields, onCreateInstanceAndAttach]);

  const handleFieldChange = useCallback(
    (instanceId: string, field: string, value: string) => {
      onSetField(instanceId, field, value);
    },
    [onSetField],
  );

  return (
    <div className="poi-editor">
      <div className="poi-editor-header">
        <h3 className="poi-editor-title">
          POIs: <span className="poi-editor-node-name">{nodeLabel}</span>
        </h3>
        <button className="poi-editor-close" onClick={onClose} title="Close">
          ✕
        </button>
      </div>

      <div className="poi-editor-body">
        {/* ── POI list ──────────────────────────────────────────── */}
        {pois.length === 0 && !showAddForm && (
          <div className="poi-editor-empty">No POIs on this node.</div>
        )}

        {pois.map((poi) => {
          const instance = findInstance(entityState.instances, poi.entity_ref);
          return (
            <div key={poi.poi_id} className="poi-item">
              <div className="poi-item-header">
                <span className="poi-item-id">{poi.poi_id}</span>
                {instance && (
                  <span className="poi-item-type">{instance.type}</span>
                )}
                <button
                  className="poi-item-delete"
                  title="Detach POI"
                  onClick={() => onDetachPoi(nodeId, poi.poi_id)}
                >
                  🗑
                </button>
              </div>
              {instance && (
                <div className="poi-item-fields">
                  {Object.entries(instance.fields).map(([field, value]) => (
                    <div key={field} className="poi-field-row">
                      <label className="poi-field-label">{field}</label>
                      <input
                        className="poi-field-input"
                        type="text"
                        value={String(value ?? '')}
                        onChange={(e) =>
                          handleFieldChange(instance.instance_id, field, e.target.value)
                        }
                      />
                    </div>
                  ))}
                  <div className="poi-field-row">
                    <label className="poi-field-label">+ field</label>
                    <input
                      className="poi-field-input poi-field-new"
                      placeholder="name"
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          const input = e.currentTarget;
                          const fieldName = input.value.trim();
                          if (fieldName) {
                            handleFieldChange(instance.instance_id, fieldName, '');
                            input.value = '';
                          }
                        }
                      }}
                    />
                  </div>
                </div>
              )}
              {!instance && (
                <div className="poi-item-noentity">
                  No entity attached —{' '}
                  <button
                    className="poi-item-link"
                    onClick={() => {
                      // Attach an existing entity by ref
                      const refId = prompt('Enter entity instance ID to link:');
                      if (refId) onAttachPoi(nodeId, poi.poi_id, refId.trim());
                    }}
                  >
                    link
                  </button>
                </div>
              )}
            </div>
          );
        })}

        {/* ── Add POI form ─────────────────────────────────────── */}
        {showAddForm && (
          <div className="poi-add-form">
            <div className="poi-add-row">
              <label className="poi-add-label">POI ID</label>
              <input
                className="poi-add-input"
                type="text"
                placeholder="e.g. boss-spawn"
                value={newPoiId}
                onChange={(e) => setNewPoiId(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
              />
            </div>
            <div className="poi-add-row">
              <label className="poi-add-label">Entity Type</label>
              <select
                className="poi-add-select"
                value={newPoiType}
                onChange={(e) => { setNewPoiType(e.target.value); setNewPoiFields({}); }}
              >
                {entityState.types.map((t) => (
                  <option key={t.name} value={t.name}>{t.name}</option>
                ))}
              </select>
            </div>
            <div className="poi-add-row">
              <label className="poi-add-label">Fields</label>
              <div className="poi-add-fields">
                <input
                  className="poi-add-field-input"
                  placeholder="field name"
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      const input = e.currentTarget;
                      const fn = input.value.trim();
                      if (fn) {
                        setNewPoiFields((prev) => ({ ...prev, [fn]: '' }));
                        input.value = '';
                      }
                    }
                  }}
                />
                {Object.entries(newPoiFields).map(([field, value]) => (
                  <div key={field} className="poi-add-field-row">
                    <span className="poi-add-field-name">{field}</span>
                    <input
                      className="poi-add-field-value"
                      type="text"
                      placeholder="value"
                      value={value}
                      onChange={(e) =>
                        setNewPoiFields((prev) => ({ ...prev, [field]: e.target.value }))
                      }
                    />
                  </div>
                ))}
              </div>
            </div>
            <div className="poi-add-actions">
              <button className="poi-add-btn" onClick={handleAdd} disabled={!newPoiId.trim()}>
                ✓ Add POI
              </button>
              <button className="poi-add-cancel" onClick={() => setShowAddForm(false)}>
                Cancel
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── Bottom action bar ──────────────────────────────────── */}
      <div className="poi-editor-footer">
        <button className="poi-editor-add-btn" onClick={() => setShowAddForm(true)}>
          ＋ Add POI
        </button>
      </div>
    </div>
  );
}
