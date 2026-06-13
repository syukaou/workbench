/** U3 topology types. Mirrors the Rust core's projection state. */

export interface POI {
  poi_id: string;
  entity_ref: string | null;
}

export interface RoomNode {
  node_id: string;
  label: string;
  x: number;
  y: number;
  marks: string[];
  pois: POI[];
}

export interface EdgeDef {
  from_node: string;
  to_node: string;
  bidirectional: boolean;
}

export interface GraphState {
  rooms: RoomNode[];
  edges: EdgeDef[];
}

/** U2 entity types and instances (from core state). */

export interface EntityTypeInfo {
  name: string;
  fields: Record<string, unknown>;
}

export interface EntityInstanceInfo {
  instance_id: string;
  type: string;
  fields: Record<string, unknown>;
}

export interface EntityState {
  types: EntityTypeInfo[];
  instances: EntityInstanceInfo[];
}

/** Actions for the undo/redo stack */
export interface GraphAction {
  type: 'add_node' | 'add_edge' | 'remove_node' | 'remove_edge' | 'toggle_edge_direction';
  payload: unknown;
}
