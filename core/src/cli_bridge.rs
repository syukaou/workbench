//! AI-CLI proposal bridge (U5 prototype).
//!
//! INV-4 note: this module spawns an external CLI agent (opencode) which
//! calls an LLM. The core crate itself does NOT embed LLM/HTTP/network code.
//! The final architecture will move this bridge into `src-tauri` as a Tauri
//! sidecar (see TECH-SPEC §5). For the U5 prototype, we place it here to
//! validate the proposal pipeline end-to-end.

use std::process::{Command as ProcessCommand, Output};

use crate::engine::Command;
use crate::error::{Error, Result};

/// Maximum raw output to retain on parse failure (for debugging).
const MAX_RAW_OUTPUT_RETAIN: usize = 4096;

// ── Proposal result ──────────────────────────────────────────────────

/// The result of running the full proposal pipeline.
#[derive(Debug, Clone)]
pub struct ProposalResult {
    /// Parsed commands from the CLI output.
    pub commands: Vec<Command>,
    /// The raw CLI stdout (for debugging / partial parse recovery).
    pub raw_output: String,
}

// ── Prompt building ──────────────────────────────────────────────────

/// Build the system prompt describing all available commands to the AI.
pub fn build_system_prompt() -> String {
    r#"You are a level topology proposer for a game design workbench.
Given a natural language description of a level layout, output a JSON array
of topology commands. Use ONLY the commands listed below.

AVAILABLE COMMANDS:

  CreateNode: { "node_id": "<id>", "label": "<display name>" }
    — Add a room/area node to the graph.

  CreateEdge: { "from_node": "<id>", "to_node": "<id>", "bidirectional": true|false }
    — Add a connection between two nodes. Set bidirectional=false for one-way passages.

  RemoveNode: { "node_id": "<id>" }
    — Remove a node from the graph.

  RemoveEdge: { "from_node": "<id>", "to_node": "<id>" }
    — Remove a connection.

  MarkNode: { "node_id": "<id>", "mark": "<semantic tag>" }
    — Apply a semantic mark to a node (e.g., "spawn", "boss", "treasure", "shortcut").

  AttachPOI: { "node_id": "<id>", "poi_id": "<poi id>", "entity_ref": "<entity id or null>" }
    — Attach a point-of-interest to a node.

  DetachPOI: { "node_id": "<id>", "poi_id": "<poi id>" }
    — Remove a POI from a node.

OUTPUT FORMAT:
- Output ONLY a valid JSON array — no explanations, no markdown fences.
- Each element is an object with a SINGLE key: the command name (e.g., "CreateNode").
- The value is the command's parameters object.
- Example:
  [
    {"CreateNode": {"node_id": "hall", "label": "Entrance Hall"}},
    {"CreateEdge": {"from_node": "hall", "to_node": "armory", "bidirectional": true}}
  ]

RULES:
- Use consistent node_id naming (short, lowercase, underscore_separated).
- A "branch" is a sequence of connected nodes that forks from a hub.
- A "shortcut" is a one-way or bidirectional edge connecting distant nodes.
- If the user asks for N branches, create N distinct paths.
- Never create orphan nodes — every node should be reachable."#
        .to_string()
}

/// Build the full user prompt for a given intent.
pub fn build_user_prompt(intent: &str) -> String {
    format!(
        "Generate a level topology for the following description:\n\n{intent}\n\nOutput ONLY the JSON array."
    )
}

/// Combine system prompt + user prompt into a single prompt for the CLI.
pub fn build_full_prompt(intent: &str) -> String {
    format!(
        "{}\n\n---\n\n{}",
        build_system_prompt(),
        build_user_prompt(intent)
    )
}

// ── CLI invocation ───────────────────────────────────────────────────

/// Call the opencode CLI with a prompt and return its raw stdout.
///
/// # Errors
/// - Returns `Error::Other` if opencode is not found.
/// - Returns `Error::Other` on non-zero exit or timeout.
pub fn call_opencode(prompt: &str) -> Result<String> {
    let output: Output = ProcessCommand::new("opencode")
        .args(["run", "--format", "json", prompt])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Other("opencode CLI not found in PATH. Install: https://opencode.ai".into())
            } else {
                Error::Other(format!("Failed to spawn opencode: {e}"))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!(
            "opencode exited with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();

    // opencode --format json outputs NDJSON (one JSON object per line).
    // The last meaningful message usually contains the final answer.
    // We extract the content from the final message.
    let extracted = extract_final_content(&raw)?;

    Ok(extracted)
}

/// Extract the final answer content from opencode's NDJSON output.
///
/// opencode --format json emits one JSON object per line. The final
/// assistant message has `type: "message"` and a `content` array.
/// We concatenate all text blocks from that message.
fn extract_final_content(raw: &str) -> Result<String> {
    let mut last_content = String::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse each line as a JSON object.
        let event: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| Error::Other(format!("Failed to parse opencode JSON line: {e}")))?;

        // Look for message events from the assistant.
        if event.get("type").and_then(|v| v.as_str()) == Some("message")
            && event.get("role").and_then(|v| v.as_str()) == Some("assistant")
        {
            // Accumulate content blocks.
            if let Some(content) = event.get("content") {
                if let Some(arr) = content.as_array() {
                    let text: String = arr
                        .iter()
                        .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join("");
                    if !text.is_empty() {
                        last_content = text;
                    }
                }
            }
        }
    }

    if last_content.is_empty() {
        return Err(Error::Other(
            "No assistant message found in opencode output".into(),
        ));
    }

    Ok(last_content)
}

// ── JSON parsing → Vec<Command> ──────────────────────────────────────

/// Parse a JSON array of command proposals into `Vec<Command>`.
///
/// The expected format (externally-tagged enum objects):
/// ```json
/// [
///   {"CreateNode": {"node_id": "hall", "label": "Entrance Hall"}},
///   {"CreateEdge": {"from_node": "hall", "to_node": "armory", "bidirectional": true}}
/// ]
/// ```
///
/// # Partial parsing
/// If some elements fail to parse, they are skipped and the successful
/// commands are returned. The caller should inspect the returned count
/// against the raw input to detect partial failures.
pub fn parse_proposals(raw: &str) -> Result<Vec<Command>> {
    // Strip markdown fences if present (AI sometimes wraps JSON in ```json ... ```).
    let cleaned = strip_markdown_fences(raw);

    // Try parsing as a JSON array first.
    let parsed: serde_json::Value = serde_json::from_str(&cleaned).map_err(|e| {
        Error::Other(format!(
            "Failed to parse CLI output as JSON: {e}\nRaw (truncated): {}",
            &cleaned[..cleaned.len().min(MAX_RAW_OUTPUT_RETAIN)]
        ))
    })?;

    let array = parsed.as_array().ok_or_else(|| {
        Error::Other(format!(
            "CLI output is not a JSON array. Got: {}",
            if parsed.is_object() {
                "object"
            } else {
                "scalar"
            }
        ))
    })?;

    // Parse each element — skip malformed ones, collect valid ones.
    let mut commands: Vec<Command> = Vec::with_capacity(array.len());
    for element in array {
        match parse_single_command(element) {
            Ok(cmd) => commands.push(cmd),
            Err(_) => {
                // Skip malformed entries in prototype mode.
                // In production, we'd accumulate errors for the caller to inspect.
            }
        }
    }

    Ok(commands)
}

/// Strip markdown code fences from a string if present.
fn strip_markdown_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    // Strip opening ```json or ```
    let inner = if let Some(rest) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        rest.trim_start()
    } else {
        trimmed
    };
    // Strip closing ```
    let inner = if let Some(rest) = inner.strip_suffix("```") {
        rest.trim_end()
    } else {
        inner
    };
    inner.to_string()
}

/// Parse a single JSON object (externally-tagged Command) into a `Command`.
///
/// Expected shape: `{"CommandVariant": { ... params ... }}`
fn parse_single_command(element: &serde_json::Value) -> Result<Command> {
    let obj = element
        .as_object()
        .ok_or_else(|| Error::Other("Command element is not a JSON object".into()))?;

    // Each element should have exactly one key (the variant name).
    if obj.len() != 1 {
        return Err(Error::Other(format!(
            "Expected exactly one key per command element, got {} keys",
            obj.len()
        )));
    }

    let (variant_name, params_value) = obj.iter().next().unwrap();

    let params = params_value
        .as_object()
        .ok_or_else(|| Error::Other("Command params is not a JSON object".into()))?;

    match variant_name.as_str() {
        "CreateNode" => {
            let node_id = get_str(params, "node_id")?;
            let label = get_str(params, "label")?;
            Ok(Command::CreateNode {
                node_id: node_id.to_string(),
                label: label.to_string(),
            })
        }
        "CreateEdge" => {
            let from_node = get_str(params, "from_node")?;
            let to_node = get_str(params, "to_node")?;
            let bidirectional = params
                .get("bidirectional")
                .and_then(|v| v.as_bool())
                .unwrap_or(true); // Default to bidirectional if missing
            Ok(Command::CreateEdge {
                from_node: from_node.to_string(),
                to_node: to_node.to_string(),
                bidirectional,
            })
        }
        "RemoveNode" => {
            let node_id = get_str(params, "node_id")?;
            Ok(Command::RemoveNode {
                node_id: node_id.to_string(),
            })
        }
        "RemoveEdge" => {
            let from_node = get_str(params, "from_node")?;
            let to_node = get_str(params, "to_node")?;
            Ok(Command::RemoveEdge {
                from_node: from_node.to_string(),
                to_node: to_node.to_string(),
            })
        }
        "MarkNode" => {
            let node_id = get_str(params, "node_id")?;
            let mark = get_str(params, "mark")?;
            Ok(Command::MarkNode {
                node_id: node_id.to_string(),
                mark: mark.to_string(),
            })
        }
        "AttachPOI" => {
            let node_id = get_str(params, "node_id")?;
            let poi_id = get_str(params, "poi_id")?;
            let entity_ref = params
                .get("entity_ref")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && s != &"null")
                .map(|s| s.to_string());
            Ok(Command::AttachPOI {
                node_id: node_id.to_string(),
                poi_id: poi_id.to_string(),
                entity_ref,
            })
        }
        "DetachPOI" => {
            let node_id = get_str(params, "node_id")?;
            let poi_id = get_str(params, "poi_id")?;
            Ok(Command::DetachPOI {
                node_id: node_id.to_string(),
                poi_id: poi_id.to_string(),
            })
        }
        // U1 / U2 commands — lower priority for topology proposal, but available.
        "Set" => {
            let key = get_str(params, "key")?;
            let value = params
                .get("value")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Ok(Command::Set {
                key: key.to_string(),
                value,
            })
        }
        "Delete" => {
            let key = get_str(params, "key")?;
            Ok(Command::Delete {
                key: key.to_string(),
            })
        }
        "CreateEntityType" => {
            let name = get_str(params, "name")?;
            Ok(Command::CreateEntityType {
                name: name.to_string(),
            })
        }
        "CreateEntityInstance" => {
            let entity_type = get_str(params, "entity_type")?;
            let instance_id = get_str(params, "instance_id")?;
            Ok(Command::CreateEntityInstance {
                entity_type: entity_type.to_string(),
                instance_id: instance_id.to_string(),
            })
        }
        "SetEntityField" => {
            let instance_id = get_str(params, "instance_id")?;
            let field = get_str(params, "field")?;
            let value = params
                .get("value")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            Ok(Command::SetEntityField {
                instance_id: instance_id.to_string(),
                field: field.to_string(),
                value,
            })
        }
        _ => Err(Error::Other(format!(
            "Unknown command variant: {variant_name}"
        ))),
    }
}

/// Extract a required string field from a JSON object.
fn get_str<'a>(obj: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> Result<&'a str> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other(format!("Missing or non-string field: {key}")))
}

// ── Full pipeline ────────────────────────────────────────────────────

/// Run the full proposal pipeline: intent → prompt → CLI → parse → commands.
///
/// On CLI failure, returns the error with context.
/// On parse failure after CLI success, the `ProposalResult` still contains
/// the raw output so the caller can inspect / retry.
pub fn propose(intent: &str) -> Result<ProposalResult> {
    let prompt = build_full_prompt(intent);

    let raw = call_opencode(&prompt)?;

    let commands = parse_proposals(&raw).unwrap_or_else(|_| {
        // Parse failure: return empty commands but preserve raw output.
        Vec::new()
    });

    Ok(ProposalResult {
        commands,
        raw_output: raw,
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parser tests (no CLI call) ───────────────────────────────────

    #[test]
    fn test_parse_create_node() {
        let json = r#"[
            {"CreateNode": {"node_id": "hall", "label": "中央大厅"}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::CreateNode { node_id, label } => {
                assert_eq!(node_id, "hall");
                assert_eq!(label, "中央大厅");
            }
            _ => panic!("Expected CreateNode"),
        }
    }

    #[test]
    fn test_parse_create_edge_bidirectional() {
        let json = r#"[
            {"CreateEdge": {"from_node": "a", "to_node": "b", "bidirectional": true}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::CreateEdge {
                from_node,
                to_node,
                bidirectional,
            } => {
                assert_eq!(from_node, "a");
                assert_eq!(to_node, "b");
                assert!(*bidirectional);
            }
            _ => panic!("Expected CreateEdge"),
        }
    }

    #[test]
    fn test_parse_create_edge_oneway() {
        let json = r#"[
            {"CreateEdge": {"from_node": "a", "to_node": "b", "bidirectional": false}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::CreateEdge { bidirectional, .. } => {
                assert!(!bidirectional);
            }
            _ => panic!("Expected CreateEdge"),
        }
    }

    #[test]
    fn test_parse_mark_node() {
        let json = r#"[
            {"MarkNode": {"node_id": "spawn_room", "mark": "spawn"}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::MarkNode { node_id, mark } => {
                assert_eq!(node_id, "spawn_room");
                assert_eq!(mark, "spawn");
            }
            _ => panic!("Expected MarkNode"),
        }
    }

    #[test]
    fn test_parse_attach_poi() {
        let json = r#"[
            {"AttachPOI": {"node_id": "boss_room", "poi_id": "boss_01", "entity_ref": "boss_dragon"}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::AttachPOI {
                node_id,
                poi_id,
                entity_ref,
            } => {
                assert_eq!(node_id, "boss_room");
                assert_eq!(poi_id, "boss_01");
                assert_eq!(entity_ref.as_deref(), Some("boss_dragon"));
            }
            _ => panic!("Expected AttachPOI"),
        }
    }

    #[test]
    fn test_parse_attach_poi_null_entity_ref() {
        let json = r#"[
            {"AttachPOI": {"node_id": "room1", "poi_id": "poi1", "entity_ref": null}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::AttachPOI { entity_ref, .. } => {
                assert_eq!(entity_ref.as_deref(), None);
            }
            _ => panic!("Expected AttachPOI"),
        }
    }

    #[test]
    fn test_parse_detach_poi() {
        let json = r#"[
            {"DetachPOI": {"node_id": "room1", "poi_id": "poi1"}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::DetachPOI { node_id, poi_id } => {
                assert_eq!(node_id, "room1");
                assert_eq!(poi_id, "poi1");
            }
            _ => panic!("Expected DetachPOI"),
        }
    }

    #[test]
    fn test_parse_remove_node() {
        let json = r#"[
            {"RemoveNode": {"node_id": "old_room"}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::RemoveNode { node_id } => {
                assert_eq!(node_id, "old_room");
            }
            _ => panic!("Expected RemoveNode"),
        }
    }

    #[test]
    fn test_parse_remove_edge() {
        let json = r#"[
            {"RemoveEdge": {"from_node": "a", "to_node": "b"}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::RemoveEdge { from_node, to_node } => {
                assert_eq!(from_node, "a");
                assert_eq!(to_node, "b");
            }
            _ => panic!("Expected RemoveEdge"),
        }
    }

    // ── U5 acceptance test: "中央大厅+三支线+单向捷径" ──────────────

    #[test]
    fn test_prototype_central_hall_three_branches_shortcut() {
        // Simulated CLI output for "生成中央大厅+三支线+单向捷径"
        let json = r#"[
            {"CreateNode": {"node_id": "central_hall", "label": "中央大厅"}},
            {"CreateNode": {"node_id": "branch_a_1", "label": "支线A-1"}},
            {"CreateNode": {"node_id": "branch_a_2", "label": "支线A-2"}},
            {"CreateNode": {"node_id": "branch_b_1", "label": "支线B-1"}},
            {"CreateNode": {"node_id": "branch_b_2", "label": "支线B-2"}},
            {"CreateNode": {"node_id": "branch_c_1", "label": "支线C-1"}},
            {"CreateNode": {"node_id": "branch_c_2", "label": "支线C-2"}},
            {"CreateEdge": {"from_node": "central_hall", "to_node": "branch_a_1", "bidirectional": true}},
            {"CreateEdge": {"from_node": "branch_a_1", "to_node": "branch_a_2", "bidirectional": true}},
            {"CreateEdge": {"from_node": "central_hall", "to_node": "branch_b_1", "bidirectional": true}},
            {"CreateEdge": {"from_node": "branch_b_1", "to_node": "branch_b_2", "bidirectional": true}},
            {"CreateEdge": {"from_node": "central_hall", "to_node": "branch_c_1", "bidirectional": true}},
            {"CreateEdge": {"from_node": "branch_c_1", "to_node": "branch_c_2", "bidirectional": true}},
            {"CreateEdge": {"from_node": "branch_a_2", "to_node": "branch_c_1", "bidirectional": false}},
            {"MarkNode": {"node_id": "central_hall", "mark": "spawn"}}
        ]"#;

        let commands = parse_proposals(json).unwrap();

        // Count by variant type
        let node_count = commands
            .iter()
            .filter(|c| matches!(c, Command::CreateNode { .. }))
            .count();
        let edge_count = commands
            .iter()
            .filter(|c| matches!(c, Command::CreateEdge { .. }))
            .count();
        let mark_count = commands
            .iter()
            .filter(|c| matches!(c, Command::MarkNode { .. }))
            .count();

        assert_eq!(
            node_count, 7,
            "Expected 7 nodes (1 hall + 3 branches × 2 nodes)"
        );
        assert_eq!(
            edge_count, 7,
            "Expected 7 edges (3 bidirectional pairs + 1 shortcut)"
        );
        assert_eq!(mark_count, 1, "Expected 1 mark (spawn)");
        assert_eq!(commands.len(), 15);

        // Verify the one-way shortcut exists
        let has_shortcut = commands.iter().any(|c| {
            matches!(c, Command::CreateEdge { from_node, to_node, bidirectional }
                if from_node == "branch_a_2" && to_node == "branch_c_1" && !bidirectional)
        });
        assert!(
            has_shortcut,
            "Expected one-way shortcut from branch_a_2 → branch_c_1"
        );

        // Verify spawn mark on central_hall
        let has_spawn = commands.iter().any(|c| {
            matches!(c, Command::MarkNode { node_id, mark }
                if node_id == "central_hall" && mark == "spawn")
        });
        assert!(has_spawn);
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn test_parse_with_markdown_fences() {
        let json = "```json\n[{\"CreateNode\": {\"node_id\": \"x\", \"label\": \"Test\"}}]\n```";

        let commands = parse_proposals(json).unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::CreateNode { node_id, label } => {
                assert_eq!(node_id, "x");
                assert_eq!(label, "Test");
            }
            _ => panic!("Expected CreateNode"),
        }
    }

    #[test]
    fn test_parse_skips_malformed_entries() {
        // Mixed valid and invalid entries — valid ones should still parse.
        let json = r#"[
            {"CreateNode": {"node_id": "good", "label": "Good"}},
            {"BadCommand": {"whatever": true}},
            {"CreateNode": {"node_id": "also_good", "label": "Also Good"}},
            "not_an_object",
            {"CreateEdge": {"from_node": "a", "to_node": "b", "bidirectional": true}}
        ]"#;

        let commands = parse_proposals(json).unwrap();
        // Only the 3 valid entries should be parsed.
        assert_eq!(commands.len(), 3);
    }

    #[test]
    fn test_parse_empty_array() {
        let json = "[]";
        let commands = parse_proposals(json).unwrap();
        assert!(commands.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let json = "not valid json at all {{{";
        let result = parse_proposals(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_not_an_array() {
        let json = r#"{"CreateNode": {"node_id": "x", "label": "Test"}}"#;
        let result = parse_proposals(json);
        assert!(result.is_err());
    }

    // ── Prompt building tests ────────────────────────────────────────

    #[test]
    fn test_build_system_prompt_includes_all_commands() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("CreateNode"));
        assert!(prompt.contains("CreateEdge"));
        assert!(prompt.contains("RemoveNode"));
        assert!(prompt.contains("RemoveEdge"));
        assert!(prompt.contains("MarkNode"));
        assert!(prompt.contains("AttachPOI"));
        assert!(prompt.contains("DetachPOI"));
    }

    #[test]
    fn test_build_user_prompt_includes_intent() {
        let intent = "生成中央大厅+三支线";
        let prompt = build_user_prompt(intent);
        assert!(prompt.contains(intent));
        assert!(prompt.contains("ONLY the JSON array"));
    }

    #[test]
    fn test_build_full_prompt_combines_both() {
        let intent = "test layout";
        let prompt = build_full_prompt(intent);
        assert!(prompt.contains("topology proposer"));
        assert!(prompt.contains(intent));
    }

    #[test]
    fn test_strip_markdown_fences_json_block() {
        let input = "```json\n[1,2,3]\n```";
        let cleaned = strip_markdown_fences(input);
        assert_eq!(cleaned, "[1,2,3]");
    }

    #[test]
    fn test_strip_markdown_fences_plain_block() {
        let input = "```\n[1,2,3]\n```";
        let cleaned = strip_markdown_fences(input);
        assert_eq!(cleaned, "[1,2,3]");
    }

    #[test]
    fn test_strip_markdown_fences_no_fences() {
        let input = "[1,2,3]";
        let cleaned = strip_markdown_fences(input);
        assert_eq!(cleaned.trim(), "[1,2,3]");
    }
}
