# Design

## Architecture and ownership

```text
AI client -- MCP stdio --> Rust/rmcp bridge -- authenticated loopback JSON --> Blender add-on
CLI -- compact JSON -----------------------------^                         main-thread timer
```

Rust owns MCP schemas, text formatting, the CLI, descriptor resolution, and socket/image I/O.
The Blender add-on owns operations, discovery contracts, permissions, and compiled snippets.
Only Blender's main thread accesses `bpy`; requests are serialized by the timer. Python exists only
inside Blender, including the small fixtures used by Rust integration tests.

The four advertised tools are inspect, discover, execute, capture. execute accepts exactly one of
code, script, steps. The shared Blender dispatch normalizes direct code/script into one Python step,
so MCP and CLI use the same validation, permissions, snippet cache, output budgets, and Undo path.
Top-level params/max_output belong to code/script; steps carry those fields per Python operation.
The catalog supplies required
arguments, permissions, defaults/ranges in argument descriptions, and the allowed operation vocabulary.
Validation uses that same required/allowed-argument metadata, with custom domain checks in the engine.
Detailed contracts are returned on demand, including several operations in one discovery call.

Existing active-scene objects are editable. `managed` is a provenance tag, not an access restriction.
Created objects enter the Compact MCP collection; shared mesh data is detached before changing material
slots. Material edits can affect every consumer of that material. Transforms use local coordinates;
new camera/light aiming targets are world coordinates. Save writes a non-overwriting copy.

## Lessons from Conduit

[Conduit's tool definitions](https://github.com/apkd/Conduit/blob/7ebbddfcfe1604ebf9bd772873b6eff464121552/Conduit.Server/Tools/UnityTools.cs)
combine broad selectors, optional returned values, reusable code, and help fetched on demand.
Its [formatter](https://github.com/apkd/Conduit/blob/7ebbddfcfe1604ebf9bd772873b6eff464121552/Conduit.Server/Tools/ToolResponseFormatter.cs)
omits empty sections and returns a single populated section without an extra heading.
Its [object inspector](https://github.com/apkd/Conduit/blob/7ebbddfcfe1604ebf9bd772873b6eff464121552/Conduit.Unity/Editor/Tools/Show/ShowTool.cs)
bounds ordinary string/collection previews. That snapshot advertises 30 tools; this project retains four.

We use those output/reuse patterns while retaining Blender's existing loopback protocol, native bpy
API, and short common operations. We do not introduce a Unity-like search language or one wrapper per
Blender feature. Rare/complex workflows use parameterized bpy snippets. New wrappers need measured value.

## Model-facing output

MCP returns one text content block, or one PNG image. There is no duplicate structuredContent and no
string-wrapper outputSchema. The CLI retains JSON for automation. Read-only hints describe inspect and
discover; execute retains MCP's conservative defaults for mutating, non-idempotent, open-world tools.

Text has an 8 KiB UTF-8 limit. Scene inspection shares column names across rows, rounds transforms to
four decimal places as before, previews collections with omitted counts, and preserves complete object
identifiers. It stops at complete rows and computes next_offset from the rows actually returned.
Scene identity and total remain in every inspection. Blender version and permissions require
context=true in MCP; CLI JSON retains both. Complete pages omit redundant returned counts.
Pagination requires an unchanged scene. Named misses are explicit. Discovery returns complete contracts
and lists any remaining operation names. A `?` suffix marks optional arguments; conditional requirements
such as exactly one of code/script remain in the argument descriptions.

Successful execution starts with `ok`; routine completion/change counts remain in CLI JSON. New script
handles and files precede optional payloads. Reused handles are omitted from MCP, using the submitted
arguments to identify reuse without retaining presentation state. A single step has no `step=0` prefix;
batches label values and new handles by zero-based step. Explicit returned values remain, including
null, false, zero, empty strings/arrays/objects. stdout is JSON-escaped and false truncation flags are omitted.

Uniform arrays of at least two nonempty scalar records use shared-header TSV; differing key order is
allowed, but key sets must match. Nested or irregular records remain compact JSON. Bare strings must be
simple ASCII identifiers and must not resemble JSON literals; other strings and headers are JSON-escaped.
Missing inspection cells use `-`; actual null and numeric strings remain distinct. Result numbers use their
JSON representation directly, preserving integer precision above 2^53 and avoiding extra float rounding.

Tables stop before an incomplete row. Oversized JSON values are omitted whole with a request to return
fewer fields/items; stdout and already-truncated script previews may have labelled UTF-8-safe prefixes.
Every omission is marked. Metadata identifiers are complete, even when the budget cannot fit every item.
Failed calls retain failed_index, completed, partial=true, the error, and available prior values/files;
bounded output still applies. Earlier changes and effects of the failed step may persist. A script's
unknown mutation count remains null in CLI JSON. MCP does not report mutation counts.

Limits are bytes/characters for predictable runtime cost, not estimates of model tokens. Rust benchmarks
count actual transcripts with named tokenizers. Wire size and provider-visible/billed tokens differ.

## Snippets and execution

Batch preparation checks every operation's syntax/permission and compiles each new script once.
Existing references are resolved before writes. dry_run neither executes nor inserts cache entries;
it does not check future object existence, Blender context viability, or filesystem access.

Each engine has a random session identity and monotonically increasing snippet IDs. The LRU holds
at most 64 compiled snippets and 1 MiB of their source sizes; these limits do not describe CPython's
total heap usage. Handles are never recycled within a session. Referenced code already prepared for
a batch remains usable within that batch even if subsequent cache insertions evict it.

Every run uses module-level code in a fresh namespace with bpy, params, output_dir. Assign result only
when feedback is needed; top-level return receives a targeted validation error. No variables or snippets persist
across bridge restarts/file loads. Explicit handle reuse runs again; expired references fail before
writes. Python permission is checked on every run and grants unrestricted local process access.

stdout and result share the script's character budget (default 2000, maximum 32000) and the batch's
32 KiB UTF-8 budget. JSON serialization validates the full result while retaining only a bounded
preview; oversized payloads are strings with truncation metadata, never malformed protocol JSON.
Arbitrary code and serialization can still consume CPU/memory or block Blender; this is not isolation.

## Transport, failures, and lifecycle

Protocol 1 sends one UTF-8 JSON line per TCP connection. Requests are capped at 256 KiB and the Rust
client accepts responses up to 2 MiB. The server binds only to 127.0.0.1 on an OS-assigned port.
Its descriptor contains the version, PID, port, credential, and export root. Descriptor publication
is atomic; Unix temporary files use mode 0600. Ambiguous instance selection fails explicitly.

Authentication precedes dispatch. The add-on retains the last 128 authenticated responses and argument
digests. A cached request ID returns its earlier response; different arguments with that ID are rejected.
This is bounded session replay protection, not durable exactly-once delivery. Neither client retries
mutations. Timeouts/disconnects report an unknown outcome and do not cancel Blender work.

The timer accepts at most eight clients, bounds receive work, dispatches one request per tick, and
expires incomplete connections after ten seconds. Operations/rendering execute synchronously.
Closing the bridge removes its own descriptor and unregisters the exact timer. Loading a file stops it.

Runtime failures stop a batch and set MCP isError. Earlier operations persist and the failed operation
may have partial effects. Interactive execution records Undo checkpoints when available; background
mode has no Undo promise, and files remain on disk. Async jobs, cancellation, persistent scripts,
scene-delta tracking, and additional top-level tools are deferred.

## Migration

Version 0.4 adds direct code/script inputs and changes MCP text presentation. Tool names, steps inputs,
CLI JSON result structure, descriptor selection, and protocol-1 transport remain. Top-level metadata
selection and direct execution need the 0.4 add-on. Deploy the bridge and add-on as a matched pair.
There is no automatic installation into or restart of an existing Blender session.

## Token budget and deferred work

Cold payload counts include schema/instructions and discovery; warm counts exclude startup context.
The v0.4 schema includes enough Python guidance to skip routine discovery, at a measured startup cost.
Regression gates cap schema/instructions at 525 cl100k_base / 540 o200k_base tokens and require at least
25% less warm repeated-code payload, 35% smaller uniform-record responses, and no cold-code regression
when the old contract needs discovery. Known-contract single calls and non-code cases are also reported.

Clear names and exact fields take precedence over shorthand that creates repair calls. The research
smoke tests found invalid top-level return and invented fields when contracts were over-compressed.
No positional input DSL, TOON conversion, new bulk-operation vocabulary, handle entropy reduction,
persistent result cache, delta tracking, or implicit stateful metadata suppression is introduced.
Capture stays explicit at 512 pixels by default; image resolution and quality need separate evaluation.
Provider prompt caching changes billing, not the number of context tokens, and is not a server-side saving.
