/**
 * WASM Core Bridge — loads the Rust workbench-core via WebAssembly
 * and exposes get_state() / execute_command() to the React frontend.
 */
import init, { get_state as wasmGetState, execute_command as wasmExecute, propose as wasmPropose, export_snapshot as wasmExportSnapshot, import_snapshot as wasmImportSnapshot } from './core-pkg/workbench_core';
// Vite will resolve this to the hashed asset URL at build time
import wasmUrl from './core-pkg/workbench_core_bg.wasm?url';

let ready = false;
let initPromise: Promise<void> | null = null;

/** Ensure the WASM module is loaded (idempotent). */
export async function ensureCore(): Promise<void> {
  if (ready) return;
  if (!initPromise) {
    initPromise = init(wasmUrl).then(() => {
      ready = true;
    });
  }
  return initPromise;
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
