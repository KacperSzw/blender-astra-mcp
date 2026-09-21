# Blender Compact MCP

Control Blender through four MCP tools: inspect, discover, execute, capture.
Optimize tokens per correctly completed task, including schemas, discovery,
arguments, results, and recovery calls.

## Repository

- src/: Rust MCP bridge (rmcp), JSON CLI, and compact text formatter.
- addon/compact_blender/: bpy operations, discovery catalog, permissions,
  authenticated loopback transport, and session snippet cache.
- nix/: reproducible builds, add-on packaging, and checks.
Keep Python confined to Blender. Use Rust for external tests and utilities.

## Interface

Keep exactly four advertised tools. execute accepts exactly one of code, script,
steps. Prefer full-bpy code and parameterized session snippets for loops or
complex work. Python is module-level: assign result only when data is needed;
no top-level return. Discover unfamiliar operations once; batch related steps.
Keep all capabilities in these tools. Cache snippets only for the bridge session.
Keep discovery and validation contracts consistent.

Return compact text through MCP and JSON through the CLI. Successful edits say
ok; new source also returns a handle. Preserve explicit values, even null/empty.
Use tables for uniform scalar records and JSON otherwise. Request fields,
pages, context metadata, or images only as needed. Emit each result once.
Bound aggregate output, mark truncation, and preserve actionable errors.
Avoid new query languages, aliases, or wrappers without measured benefit.

## Correctness

Only Blender's main thread may access bpy. Blender owns permissions.
Python execution is unrestricted local code and bypasses narrower toggles.
Prevalidate batches; runtime failures can leave partial changes.
dry_run checks syntax and permissions, not transactional success.
Timeout does not cancel work. Never automatically replay mutations.
Keep credentials out of results and logs.

## Verification

Run cargo fmt, cargo clippy, cargo test, and nix flake check.
Set BLENDER_EXE for headless integration tests; UI checks are opt-in.
Use isolated factory-startup Blender processes, never a user's scene.
Measure cold and warm task transcripts with named tokenizers. Report schema,
discovery, argument, result, and image costs separately.
Keep AGENTS.md compact; put detailed contracts and evidence in project docs.
