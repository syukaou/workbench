import type { GraphState, RoomNode, EdgeDef } from './types';

/** Mock graph state: 5 rooms with central↔three-branch topology. */
export function loadMockState(): GraphState {
  const rooms: RoomNode[] = [
    {
      node_id: 'entrance',
      label: 'Entrance Hall',
      x: 400,
      y: 300,
      marks: ['spawn'],
      pois: [
        { poi_id: 'entrance-door', entity_ref: null },
      ],
    },
    {
      node_id: 'armory',
      label: 'Armory',
      x: 150,
      y: 150,
      marks: [],
      pois: [
        { poi_id: 'weapon-rack', entity_ref: null },
      ],
    },
    {
      node_id: 'library',
      label: 'Library',
      x: 650,
      y: 150,
      marks: [],
      pois: [
        { poi_id: 'scroll-table', entity_ref: null },
      ],
    },
    {
      node_id: 'garden',
      label: 'Garden',
      x: 400,
      y: 80,
      marks: [],
      pois: [],
    },
    {
      node_id: 'vault',
      label: 'Vault',
      x: 150,
      y: 30,
      marks: ['shortcut'],
      pois: [
        { poi_id: 'locked-chest', entity_ref: null },
      ],
    },
  ];

  const edges: EdgeDef[] = [
    { from_node: 'entrance', to_node: 'armory', bidirectional: true },
    { from_node: 'entrance', to_node: 'library', bidirectional: true },
    { from_node: 'entrance', to_node: 'garden', bidirectional: true },
    { from_node: 'armory', to_node: 'vault', bidirectional: false }, // one-way armory → vault
    { from_node: 'library', to_node: 'garden', bidirectional: false }, // one-way library → garden
  ];

  return { rooms, edges };
}

/** Find a node by id. */
export function findRoom(rooms: RoomNode[], nodeId: string): RoomNode | undefined {
  return rooms.find((r) => r.node_id === nodeId);
}

/** Find an edge by from/to ids. */
export function findEdge(edges: EdgeDef[], from: string, to: string): EdgeDef | undefined {
  return edges.find((e) => e.from_node === from && e.to_node === to);
}
