# Blender Compact MCP for Astra

Four tools for Blender: **inspect**, **discover**, **execute**, **capture**.
Batch related edits, discover unfamiliar operation contracts, and request only the feedback you need.

![50 cubes created in a batch and rendered in Blender](examples/demo.png)

Version **0.4.0** adds direct `execute` code and script inputs, smaller receipts, and shared-column
result tables. The bridge, CLI, and tests use Rust; Python stays inside Blender for `bpy`.
MCP returns compact text or a PNG; the CLI retains JSON.
The operation vocabulary covers existing-scene editing, primitives, materials, linked arrays,
modifiers, RNA properties, keyframes, frame evaluation, rendering, and saving copies.
Direct code provides the full Blender API for nodes, rigging, simulation, import/export, and more.

## Install

Requirements: Blender 4.2+; the tested build target is Nix on x86_64-linux with Blender 5.1.1.
The bridge needs no external Python or Node runtime.

```sh
nix build .#blender-compact-mcp --out-link result-bridge
nix build .#addon --out-link result-addon
```

Install `result-addon/compact_blender-0.4.0.zip` through Blender's **Preferences → Add-ons → Install from Disk**.
Enable **Compact Blender MCP**, then open the **Compact MCP** tab in the 3D Viewport sidebar (`N`).
Review permissions and select **Start bridge**. Permissions are fixed until the bridge is stopped/restarted.

Configure your MCP client with the absolute path to the built command:

```json
{
  "mcpServers": {
    "blender-compact": {
      "command": "/absolute/path/result-bridge/bin/blender-compact-mcp"
    }
  }
}
```

The bridge reads its credential from a local descriptor; never paste it into model context.
For multiple Blender instances, set `BLENDER_COMPACT_CONNECTION` to the intended `connection-<pid>.json`:
Windows `%LOCALAPPDATA%/blender-compact-mcp/`; Linux/macOS
`${XDG_STATE_HOME:-~/.local/state}/blender-compact-mcp/`.
Ambiguous discovery fails. Normal shutdown removes the descriptor; loading another `.blend` stops the bridge.

## Four tools

| Tool | Input and feedback |
|---|---|
| `inspect` | `names` selects exact objects; `match` is a case-sensitive name glob. `fields` projects properties. Default page size 20, maximum 100, with `offset` pagination. `context=true` adds Blender version and permissions. |
| `discover` | Omit `operation` to list summaries, or pass one name/a list of up to 16 names for contracts. `?` marks an optional field; the header gives the permission. |
| `execute` | Exactly one of `code`, `script`, or `steps`. Steps batch up to 100 `{op,...arguments}` operations and 500 expansion units. Returns `ok`, requested data/files, or a partial-failure report. |
| `capture` | PNG from `view="camera"` or an interactive `"viewport"`; `size` defaults to 512, range 64–1024. |

`names` and `match` are mutually exclusive. `fields` accepts `name`, `type`, `managed`, `location`,
`rotation`, `scale`, `vertices`, `polygons`, and `materials`; `name` is always included.
Without `fields`, scene pages retain the original summary fields and named inspection includes details.
Mesh fields apply only to meshes. Missing requested names are reported. Pagination assumes an unchanged scene.

Discover common operations together, then batch the edits:

```json
{"operation":["primitive","array","material","assign_material"]}
```

```json
{"steps":[{"op":"primitive","kind":"cube","name":"Demo","location":[0,0,1]},{"op":"array","name":"Demo","count":49,"offset":[2.5,0,0],"prefix":"Copy"}]}
```

Successful edits without returned data say `ok`. New code also returns a reusable handle.
Completion/change counters remain in CLI JSON. No scene dump or screenshot is implicit.
MCP text is capped at 8 KiB. Inspection stops at whole rows and supplies `next_offset`; discovery
lists remaining operation names when contracts do not fit. Collections show bounded previews with counts.
Uniform arrays of scalar records use tables with shared headers. Nested/irregular data uses compact
JSON. Ambiguous strings are JSON-quoted; large integers stay exact. Oversized payloads are explicitly
marked; table rows and script/file identifiers are never cut in half. The CLI returns the underlying JSON.

## Reusable Blender snippets

Submit module-level Python directly; no operation discovery is needed. Use loops for repeated work,
filter/aggregate inside Blender, and return only data the task needs:

```json
{"code":"bpy.data.objects[params['name']].location.z=params['z']","params":{"name":"Demo","z":3}}
```

The receipt contains `ok` and `script=<opaque handle>`. Reuse the handle with new parameters:

```json
{"script":"<handle from receipt>","params":{"name":"Demo","z":5}}
```

This reuse returns just `ok`; the handle is not echoed. Each run has a fresh namespace containing
`bpy`, `params`, and `output_dir`. Assign `result` only to return needed JSON-compatible data;
omit it for edits with no requested data. A top-level `return` is invalid. Explicit `null`, `false`,
zero, empty strings, arrays, and objects are preserved. For example:

```json
{"code":"result=bpy.context.scene.frame_current"}
```

The result is `ok`, a new `script` handle, and `result: 1` (if the current frame is 1).
Script change counts are unknown (`null` in CLI JSON).
The cache stores compiled code, not variables, and is limited to 64 entries/1 MiB of source using LRU
eviction. Restarting the Blender bridge invalidates handles. Explicit reuse runs the code again.

`max_output` defaults to 2,000 combined stdout/result characters, with a maximum of 32,000.
All scripts in a batch share an additional 32 KiB UTF-8 output budget. Truncation does not cancel edits.
`params` and `max_output` apply to direct `code`/`script` calls. With `steps`, put them inside each
`{"op":"python",...}` step. That existing form remains available for mixed batches. The three
top-level modes are mutually exclusive, including when one supplied value is `null`.

## CLI and failure semantics

```sh
blender-compact discover --params '{"operation":["primitive","array"]}'
blender-compact inspect --params '{"match":"Copy_*","fields":["location"],"limit":5}'
blender-compact execute --params - < examples/scene.json
blender-compact execute --params '{"code":"result=bpy.context.scene.frame_current"}'
blender-compact capture --params '{"filename":"preview.png","size":768}'
```

Use `--connection` for an explicit descriptor and `--timeout` for the CLI wait (default 180 seconds).
MCP `execute` accepts `timeout` from 1 to 86,400 seconds. A timeout does **not** cancel Blender work;
the outcome may be unknown. Inspect before retrying; mutations are never automatically replayed.

All step syntax and permissions are checked before writes. `dry_run=true` performs only those checks.
Existence, Blender context, and filesystem checks can still fail at runtime. Completed operations and
their returned values/files survive a later failure; the failing operation may also have partial effects.
MCP marks failures with `isError=true`. The CLI exits nonzero and keeps partial batch results on stdout;
transport/prevalidation errors are JSON on stderr. Interactive Undo is supported; files are not undone.

All five permission toggles default on for this workstation fork. **Python is unrestricted local code
execution and can bypass the narrower write/render/delete/save toggles.** It is not a sandbox.
Save/capture refuse existing output filenames. See [security](SECURITY.md).

## Development and measurement

```sh
nix develop
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
BLENDER_EXE=$(command -v blender) cargo test --locked
nix flake check
```

Nix checks include isolated headless Blender, ZIP installation, actual MCP stdio, and token regression
tests. They do not connect to an existing scene or save user preferences. Without `BLENDER_EXE`, local
Blender tests report that they were skipped. UI/Undo/viewport testing is separate and opt-in:

```sh
BLENDER_EXE=$(command -v blender) BLENDER_TEST_UI=1 BLENDER_TEST_WORKSPACE=<native-workspace-id> \
  cargo test --test ui -- --ignored
```

The UI test uses `workstation-desktop launch --background` and closes only its own test Blender instance.
Use the native workspace containing your terminal. All other checks work without that workstation helper.

The benchmark compares recorded **v0.3** calls with actual **v0.4** calls for nine matched tasks, checks
scene outcomes, and counts schemas, instructions, discovery, arguments, text results, and normalized MCP wire data.
It uses `cl100k_base` and `o200k_base`; it does not measure provider billing or image costs.
Direct code saves discovery and repeated wrappers, while the larger schema costs more at startup.
Both a caller needing Python discovery and one already knowing the old contract are measured.
Run `cargo test --test benchmark` with `BLENDER_EXE` to generate ignored `artifacts/` reports.
See [validation](docs/validation.md), [examples](docs/tool-examples.md), and [design](docs/design.md). The original 47.77% batching-only
measurement is retained in [the historical report](docs/benchmark.json), not presented as a product comparison.

The package includes both binaries and add-on source. The separate add-on output contains the ZIP and
SHA256 checksum. MIT licensed; independent software, not affiliated with Blender or a model provider.
