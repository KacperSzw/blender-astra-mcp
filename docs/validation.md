# Validation — 2026-09-21

Version **0.4.0**. Runtime: NixOS x86_64-linux, Blender **5.1.1**, Rust **1.95.0**,
official Rust MCP SDK `rmcp` **3.4.0**. Python runs only inside Blender.

## Executed checks

- Rust suite: **23 passed**, with one interactive test ignored by default. Integration tests
  use isolated factory-startup Blender processes and temporary configuration.
- `cargo fmt --check`, Clippy with warnings denied, Ruff checks/formatting, and `nix flake check`
  passed. The Nix package check runs the Rust suite with real headless Blender. The add-on ZIP
  installs and enables in an isolated Blender scripts directory.
- Separate opt-in GUI test passed on the originating terminal's native workspace through
  `workstation-desktop launch --background`: direct-code transform, viewport PNG, Undo restoring
  the previous transform, timer removal, and descriptor cleanup. Only its own Blender instance closed.
- Actual MCP stdio verified exactly four tools, direct code/script and existing steps inputs,
  no outputSchema or duplicate structuredContent, compact text, partial failures, PNG content,
  optional inspection metadata, and actionable validation errors. CLI tests verified unchanged
  JSON receipts/counters, returned handles on reuse, stdin input, and nonzero failure status.
- Blender tests cover all 16 operations, mixed-mode rejection, syntax/permission prevalidation,
  direct-code dry runs without execution/cache insertion, cache reuse/eviction/restart, Unicode
  output budgets, retained earlier results/files after failure, and non-JSON result errors.
- Formatting checks cover explicit null/false/zero/empty values, differing record-key order,
  nested/irregular data, missing cells, numeric strings, escaped/Unicode names, complete-row
  pagination/truncation, and integer precision above both 2^53 and 64 bits. Blender-to-Rust
  transport also preserves the exact value of `10**50+1`.
- Existing transport tests cover authentication, framing and size limits, replay conflicts/cache
  bounds, ambiguous descriptors, timeout uncertainty, no automatic retry, and PNG path restrictions.

Nix uses a filtered snapshot of the complete working tree because Git flake evaluation excludes
untracked Rust files. No live add-on installation, user scene, preferences, or system configuration
was changed. Release artifacts are built locally; no commit or remote release was published.

## Matched task measurements

[Raw measurements](benchmark-v0.4.json) compare frozen v0.3 schemas and actual stdio transcripts
in `tests/fixtures/v0.3-*.json` with actual v0.4 stdio calls. Both use the same isolated scene
setup and verify task outcomes. `tests/benchmark.rs` regenerates transcripts, schemas, and reports
under ignored `artifacts/`. Each column below records one run; random session handles affect counts.

Cold text payload includes instructions, advertised schemas, arguments, and content text.
Warm payload excludes schema/instructions. Each component is tokenized separately using
`cl100k_base` and `o200k_base` from `tiktoken-rs` 0.12. The report separates discovery arguments
and results from other calls, and reports normalized JSON-RPC wire costs separately.

| Task | Calls v0.3 → v0.4 | cl100k cold | cl100k warm | o200k cold | o200k warm |
|---|---:|---:|---:|---:|---:|
| Discover and edit once | 2 → 2 | 522 → 589 | 73 → 64 | 534 → 600 | 73 → 64 |
| Batch 50 transforms | 1 → 1 | 1458 → 1528 | 1009 → 1003 | 1421 → 1490 | 960 → 954 |
| Discover four contracts | 1 → 1 | 630 → 706 | 181 → 181 | 644 → 719 | 183 → 183 |
| Inspect two locations | 1 → 1 | 522 → 574 | 73 → 49 | 534 → 583 | 73 → 47 |
| Discover Python and run eight parameterized edits | 9 → 8 | 1196 → 953 | 747 → 428 | 1208 → 956 | 747 → 420 |
| Partial failure and inspection | 2 → 2 | 611 → 646 | 162 → 121 | 623 → 656 | 162 → 120 |
| One code call requiring old-contract discovery | 2 → 1 | 678 → 631 | 229 → 106 | 690 → 641 | 229 → 105 |
| One code call with old contract already known | 1 → 1 | 569 → 631 | 120 → 106 | 581 → 641 | 120 → 105 |
| Return 20 uniform records | 1 → 1 | 835 → 765 | 386 → 240 | 829 → 757 | 368 → 221 |

Repeated-code warm payload is **42.7% / 43.8% smaller** in this run. The uniform-record response
alone falls from **328 to 189** / **309 to 170** tokens (**42.4% / 45.0% smaller**). A targeted
inspection response falls from **57 to 33 / 31** tokens, and a no-data edit receipt from **7 to 1**.
The 50-transform case deliberately keeps its original steps; converting existing Python to Python
is not credited with loop-versus-50-explicit-operations savings.

Startup schema/instruction overhead rises from **449 to 525** / **461 to 536** tokens to expose
direct Python and explain pagination/failure semantics. A single cold known-contract call
and the non-code cold cases therefore regress, despite smaller responses. The benefit targets
code-first repeated work and callers who would otherwise discover the Python operation.

Regression gates require schema/instructions ≤525/540 tokens, ≥25% less warm repeated-code payload,
≥35% smaller uniform-record responses, and no cold-code regression when old-contract discovery is
needed. All passed. These are offline payload measurements, **not billed/provider token usage**.
Scene setup/verification, model prompts/reasoning/history, provider tool loading/caching, and images
are excluded. Capture defaults remain 512 pixels; image costs/quality need a separate evaluation.

## Luna readability and recovery smoke tests

Fresh **gpt-5.6-luna** agents received the advertised schema and small task prompts. These were
prompt-only trials whose candidate calls were replayed through real MCP/Blender, not native
function-calling accuracy tests or a statistical model comparison. [Recorded evidence](luna-v0.4.json)
includes the initial answers, corrected guidance trial, and actual calls/results with payload counts.

- Four of five initial schema-only calls were valid: a frame query, a no-result loop over 20 objects,
  script reuse, and a dry run. The fifth invented `primitive_add` during discovery. Actual recovery
  required a failed request, listing operation names, and then fetching `primitive`/`array` contracts;
  all three discovery calls are included in the recorded costs.
- A second trial preserved large integers and string/bool/null types, understood `ok` and handle
  omission, and produced valid repairs for top-level return and incorrect primitive/array fields.
  Its pagination answer prematurely allowed stopping on a short page, and its failure interpretation
  did not clearly preserve possible effects of the failing step.
- Schema guidance was tightened within the budget: follow next_offset until absent in an unchanged
  scene, and failed steps may leave changes. A fresh third trial correctly interpreted both cases
  and omitted an unnecessary result assignment for a no-data edit.
- Replayed mutation/query candidates passed in isolated Blender after recovery; object positions,
  linked-copy count, unchanged frame after dry run, and parameterized reuse outcomes were checked.
  The supplied example handle was rebound to equivalent fixture code in the isolated test session.

The failures are retained rather than counted as first-attempt successes. The sample is small and
does not establish autonomous modeling quality. Exact operation fields and readable result types
remain more useful than aggressively shortened descriptions that cause additional recovery calls.

## Limits and history

Blender 4.2 compatibility and Windows/macOS Rust binaries are not runtime-tested; the exercised
target is x86_64-linux/Blender 5.1.1. This is not a security audit or sustained load test. Python is
unrestricted local code. Timeout does not cancel operations; partial changes/files persist; background
mode has no Undo guarantee. Oversized responses retain explicit truncation instead of complete data.

The [v0.3 validation](validation-v0.3.md), [v0.3 measurements](benchmark-v0.3.json),
[v0.2 validation](validation-v0.2.md), and [original batching benchmark](benchmark.json) remain
historical evidence. No comparison against another Blender MCP product was measured.
