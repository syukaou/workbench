//! Entry point for `workbench-server` — the AI proposal HTTP server.
//!
//! Starts a minimal HTTP server on localhost:5198 (POST /propose).
//! Requires the `native` feature (compiled with rusqlite + CLI bridge).
//! Press Ctrl+C to stop.

fn main() {
    workbench_core::cli_server::run_server();
}
