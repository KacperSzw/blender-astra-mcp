# Validation — 2026-09-21

Version **0.3.0**. Runtime: NixOS x86_64-linux, Blender **5.1.1**, Rust **1.95.0**,
official Rust MCP SDK `rmcp` **3.4.0**. Python runs only inside Blender.

## Executed checks

- `cargo fmt --check`, Clippy with warnings denied, and Ruff checks/formatting passed.
- Rust suite: **18 passed**, with the one interactive test explicitly ignored by default.
  Integration tests used isolated factory-startup Blender processes and temporary configuration.
- `nix flake check` passed all three checks: the release bridge package (including the Rust suite
  with real headless Blender), installable add-on ZIP, and Blender Python style checks.
- Separate opt-in GUI test passed on the originating terminal's native workspace using
  `workstation-desktop launch --background`: timer-driven edits, viewport PNG, Undo restoring
  the previous transform, timer removal, and descriptor cleanup. Only its own Blender instance closed.
- Actual MCP stdio initialization and calls verified exactly four tools, no outputSchema or duplicate
  structuredContent, compact text, partial failures with isError, and PNG content. CLI checks verified
  JSON, stdin arguments, and failure exit status.
- Blender tests exercised all 16 operations, existing-scene editing, shared-mesh material isolation,
  syntax/permission prevalidation, dry runs, partial effects with retained earlier results/files,
  filtered/projected/paged inspection, save copies, camera capture, and CPU Cycles rendering.
- Snippet tests verified parameter reuse, restart invalidation, count/source-byte
  eviction, per-run permissions, aggregate UTF-8 output limits, Unicode truncation, and runtime or
  non-JSON result failures. Transport checks covered authentication, fragmented messages, request
  and response limits, replay conflicts/cache bounds, ambiguous descriptors, timeout uncertainty,
  no automatic retry, and PNG path restrictions.
- The add-on ZIP was installed and enabled in an isolated Blender scripts directory; registration,
  bridge start/stop, timer lifecycle, and descriptor removal passed.

The Nix check used a filtered snapshot of the complete working tree, including new Rust files;
Git-based flake evaluation excludes files that have not yet been added to Git. No live installation,
user scene, user preferences, or shared workstation configuration was changed.

## Matched task measurements

[Raw measured results](benchmark-v0.3.json) compare recorded v0.2 MCP calls at commit `90a1f24`
with actual v0.3 stdio calls. Both used the same isolated factory scene/setup and verified task
outcomes. Baseline schemas and transcripts are checked into `tests/fixtures/`; current transcripts
and reports are regenerated in ignored `artifacts/` by `tests/benchmark.rs`.

Cold text payload includes instructions, advertised schemas, arguments, and text content.
Warm payload excludes instructions and schemas. Each component is tokenized separately; these
are offline payload measurements, **not provider usage or billed savings**. Normalized MCP wire
counts additionally include tool-call envelopes and the baseline's duplicate structuredContent.
Clients may expose one or both copies to the model, so wire savings are reported separately.
Images, scene setup, model reasoning/history, provider-specific tool loading, and billing/cache
behavior are excluded. No model was invoked by the benchmark.

| Task | Calls v0.2 → v0.3 | cl100k cold | cl100k warm | o200k cold | o200k warm |
|---|---:|---:|---:|---:|---:|
| Discover and edit once | 2 → 2 | 565 → 522 | 86 → 73 | 578 → 534 | 86 → 73 |
| Batch 50 transforms | 1 → 1 | 1497 → 1458 | 1018 → 1009 | 1461 → 1421 | 969 → 960 |
| Discover four contracts | 4 → 1 | 673 → 630 | 194 → 181 | 686 → 644 | 194 → 183 |
| Inspect two objects' locations | 1 → 1 | 669 → 522 | 190 → 73 | 680 → 534 | 188 → 73 |
| Discover Python and run eight parameterized edits | 9 → 9 | 1476 → 1196 | 997 → 747 | 1505 → 1208 | 1013 → 747 |
| Partial failure and inspection | 2 → 2 | 676 → 611 | 197 → 162 | 690 → 623 | 198 → 162 |

Encodings are `cl100k_base` and `o200k_base` from `tiktoken-rs` 0.12. Schema/instruction
overhead decreased from 479 to 449 tokens and 492 to 461 tokens respectively. Snippet measurements
vary slightly with random session handles; the table records one actual run. Tests gate schema
overhead and the cold edit against regressions, and require warm discovery/snippet improvements.
These small matched tasks do not establish autonomous modeling quality or long-session costs.

The [v0.2 validation report](validation-v0.2.md) and [original batching-only benchmark](benchmark.json)
are retained as historical evidence. There is no measured comparison against another Blender MCP product.

## Limits

Blender 4.2 compatibility and the Rust bridge on Windows/macOS are not runtime-tested in this release;
the exercised package target is x86_64-linux/Blender 5.1.1. This is not a security audit or sustained
load test. Python is unrestricted local code. Timeout does not cancel operations, failed batches
can leave partial changes, background mode has no Undo guarantee, and files are not transactional.
