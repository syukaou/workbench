/* @ts-self-types="./workbench_core.d.ts" */

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
 * @param {string} json_str
 * @returns {any}
 */
export function execute_command(json_str) {
    const ptr0 = passStringToWasm0(json_str, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.execute_command(ptr0, len0);
    return ret;
}

/**
 * Export a full project snapshot as JSON (events + state).
 *
 * Returns a JSON object:
 * ```json
 * {"version": 1, "events": [...], "state": {...}}
 * ```
 * The frontend can save this as a `.workbench.json` file.
 * @returns {any}
 */
export function export_snapshot() {
    const ret = wasm.export_snapshot();
    return ret;
}

/**
 * Get the current materialized state as a JSON string.
 *
 * Returns a JSON object mapping namespace-prefixed keys to values.
 * The state is derived by folding all events in the in-memory log.
 * @returns {any}
 */
export function get_state() {
    const ret = wasm.get_state();
    return ret;
}

/**
 * Import a project snapshot, replacing the current state.
 *
 * The JSON must contain an `events` array of serialized events.
 * Returns `{"ok": true}` on success, or `{"ok": false, "error": "..."}` on failure.
 * @param {string} json_str
 * @returns {any}
 */
export function import_snapshot(json_str) {
    const ptr0 = passStringToWasm0(json_str, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.import_snapshot(ptr0, len0);
    return ret;
}

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
 * @param {string} intent
 * @returns {any}
 */
export function propose(intent) {
    const ptr0 = passStringToWasm0(intent, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.propose(ptr0, len0);
    return ret;
}

/**
 * Redo `count` events. Returns `{"ok":true,"redone":N}`.
 * @param {number} count
 * @returns {any}
 */
export function redo(count) {
    const ret = wasm.redo(count);
    return ret;
}

/**
 * Redo all remaining events. Returns `{"ok":true,"redone":N}`.
 * @returns {any}
 */
export function redo_all() {
    const ret = wasm.redo_all();
    return ret;
}

/**
 * Undo `count` events via the event log. Returns `{"ok":true,"undone":N}`.
 * @param {number} count
 * @returns {any}
 */
export function undo(count) {
    const ret = wasm.undo(count);
    return ret;
}

/**
 * Undo all events back to seq 0. Returns `{"ok":true,"undone":N}`.
 * @returns {any}
 */
export function undo_all() {
    const ret = wasm.undo_all();
    return ret;
}

/**
 * Undo/redo cursor status: `{"current_seq":S,"total_events":T}`.
 * The UI derives canUndo = current_seq > 0, canRedo = current_seq < total_events.
 * @returns {any}
 */
export function undo_redo_status() {
    const ret = wasm.undo_redo_status();
    return ret;
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_ea4887a5f8f9a9db: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_now_d2e0afbad4edbe82: function() {
            const ret = Date.now();
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./workbench_core_bg.js": import0,
    };
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('workbench_core_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
