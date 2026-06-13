/* tslint:disable */
/* eslint-disable */

/**
 * Execute a command from a JSON string.
 *
 * The JSON must be an externally-tagged Command:
 * ```json
 * {"CreateNode": {"node_id": "hall", "label": "Entrance Hall"}}
 * ```
 *
 * Returns a JSON object: `{"ok": true, "seq": N}` on success,
 * or `{"ok": false, "error": "message"}` on failure.
 */
export function execute_command(json_str: string): any;

/**
 * Export a full project snapshot as JSON (events + state).
 *
 * Returns a JSON object:
 * ```json
 * {"version": 1, "events": [...], "state": {...}}
 * ```
 * The frontend can save this as a `.workbench.json` file.
 */
export function export_snapshot(): any;

/**
 * Get the current materialized state as a JSON string.
 *
 * Returns a JSON object mapping namespace-prefixed keys to values.
 * The state is derived by folding all events in the in-memory log.
 */
export function get_state(): any;

/**
 * Import a project snapshot, replacing the current state.
 *
 * The JSON must contain an `events` array of serialized events.
 * Returns `{"ok": true}` on success, or `{"ok": false, "error": "..."}` on failure.
 */
export function import_snapshot(json_str: string): any;

/**
 * Generate topology proposals from a natural language intent.
 *
 * In WASM, we use a mock proposal generator (CLI is native-only, INV-4).
 * Returns a JSON array of command objects ready for `execute_command`.
 *
 * The mock generator parses keywords from the intent:
 * - "branch" / "fork" → creates hub with branch nodes
 * - "loop" / "circle" → creates a cycle
 * - "shortcut" → creates a one-way shortcut edge
 * - "secret" → creates a hidden room with one-way entrance
 * - Otherwise → creates a simple linear chain
 */
export function propose(intent: string): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly execute_command: (a: number, b: number) => any;
    readonly import_snapshot: (a: number, b: number) => any;
    readonly propose: (a: number, b: number) => any;
    readonly export_snapshot: () => any;
    readonly get_state: () => any;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
