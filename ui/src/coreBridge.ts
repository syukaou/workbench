/**
 * WASM Core Bridge — loads the Rust workbench-core via WebAssembly
 * and exposes get_state() / execute_command() to the React frontend.
 */
import init, { get_state as wasmGetState, execute_command as wasmExecute, propose as wasmPropose, export_snapshot as wasmExportSnapshot, import_snapshot as wasmImportSnapshot, undo as wasmUndo, redo as wasmRedo, undo_redo_status as wasmUndoRedoStatus } from './core-pkg/workbench_core';
// Vite will resolve this to the hashed asset URL at build time
import wasmUrl from './core-pkg/workbench_core_bg.wasm?url';

let ready = false;
let initPromise: Promise<void> | null = null;

/** Ensure the WASM module is loaded (idempotent). */
export async function ensureCore(): Promise<void> {
  if (ready) return;
  if (!initPromise) {
    // wasm-bindgen's init() takes a single options object; the positional-URL
    // form is deprecated (logs a console warning on every load).
    initPromise = init({ module_or_path: wasmUrl }).then(() => {
      ready = true;
    });
  }
  return initPromise;
}

/** Whether the WASM core has finished initializing (module-level truth). */
export function isCoreReady(): boolean {
  return ready;
}

/** Get the current core state as a parsed JSON object. */
export function getCoreState(): Record<string, unknown> {
  if (!ready) {
    throw new Error('Core not initialized. Call ensureCore() first.');
  }
  const json = wasmGetState();
  // wasmGetState returns a JsValue (stringified JSON)
  return JSON.parse(json as string);
}

/** Execute a command on the core and return the result. */
export function executeCoreCommand(cmdObj: Record<string, unknown>): {
  ok: boolean;
  seq?: number;
  error?: string;
} {
  if (!ready) {
    return { ok: false, error: 'Core not initialized. Call ensureCore() first.' };
  }
  const result = wasmExecute(JSON.stringify(cmdObj));
  // wasmExecute returns a JsValue (stringified JSON)
  return JSON.parse(result as string);
}

/** Generate topology proposals from a natural language intent. */
export function proposeViaCore(intent: string): Record<string, unknown>[] {
  if (!ready) {
    throw new Error('Core not initialized. Call ensureCore() first.');
  }
  const result = wasmPropose(intent);
  // wasmPropose returns a JsValue (stringified JSON array)
  return JSON.parse(result as string) as Record<string, unknown>[];
}

// ── v1.4: Save/Load persistence ─────────────────────────────────────

/** Export a full project snapshot from the WASM core as a JSON string. */
export function exportCoreSnapshot(): string {
  if (!ready) {
    throw new Error('Core not initialized. Call ensureCore() first.');
  }
  return wasmExportSnapshot() as string;
}

/** Import a project snapshot into the WASM core, replacing current state. */
export function importCoreSnapshot(jsonStr: string): { ok: boolean; error?: string } {
  if (!ready) {
    return { ok: false, error: 'Core not initialized. Call ensureCore() first.' };
  }
  const result = wasmImportSnapshot(jsonStr);
  return JSON.parse(result as string);
}

// ── core-as-truth: undo/redo via the event log (INV-1/INV-5) ────────
// The core IS the single source of truth for history. The UI never keeps
// its own undo stack — it asks the core to walk the event-log cursor and
// then re-reads get_state(). canUndo/canRedo derive from the cursor below.

/** Undo `count` events through the core event log. */
export function coreUndo(count = 1): { ok: boolean; undone?: number; error?: string } {
  if (!ready) {
    return { ok: false, error: 'Core not initialized. Call ensureCore() first.' };
  }
  return JSON.parse(wasmUndo(count) as string);
}

/** Redo `count` events through the core event log. */
export function coreRedo(count = 1): { ok: boolean; redone?: number; error?: string } {
  if (!ready) {
    return { ok: false, error: 'Core not initialized. Call ensureCore() first.' };
  }
  return JSON.parse(wasmRedo(count) as string);
}

/**
 * Undo/redo cursor from the core: `{ current_seq, total_events }`.
 * canUndo = current_seq > 0, canRedo = current_seq < total_events.
 */
export function coreUndoRedoStatus(): { current_seq: number; total_events: number } {
  if (!ready) {
    return { current_seq: 0, total_events: 0 };
  }
  return JSON.parse(wasmUndoRedoStatus() as string);
}
