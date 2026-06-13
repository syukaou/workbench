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
 * Get the current materialized state as a JSON string.
 *
 * Returns a JSON object mapping namespace-prefixed keys to values.
 * The state is derived by folding all events in the in-memory log.
 */
export function get_state(): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly execute_command: (a: number, b: number) => any;
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
