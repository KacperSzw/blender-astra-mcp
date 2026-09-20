# Blender Compact MCP for Astra

Four tools for Blender: **inspect**, **discover**, **execute**, **capture**.
Batch related edits, fetch operation arguments only when needed, and return bounded summaries.

![50 managed cubes created in a batch and rendered in Blender](examples/demo.png)

Version 0.2 adds existing-scene editing and an unrestricted Python operation for the full
Blender API, while keeping four MCP tools and on-demand operation discovery.

## What works

- Existing object transforms and material editing, primitives, linked arrays, cameras and lights.
- Batched modifiers, RNA properties, keyframes, frame evaluation and RNA discovery.
- Full Python access for Geometry Nodes, rigging, simulations, import/export and other bpy workflows.
  Discover `python`, then execute `{"op":"python","code":"..."}`. Assign `result` for a bounded JSON
  response; `bpy`, `params` and `output_dir` are available in the script.
- Camera or interactive viewport previews; `render` uses current scene resolution and supports animation.
- Independent write/render/delete/save toggles and an additional Python toggle, fixed at bridge start.
  **Python is unrestricted local code execution and can bypass all the narrower toggles.** All permissions
  default on in this workstation-oriented fork.
- Interactive undo checkpoints and explicit partial batch errors. Long operations block Blender;
  the MCP execute timeout is configurable and does not cancel work.

The new capabilities are not fully validated. External asset-provider integrations are not bundled.
The historical benchmark below measures batching only; no cross-product token comparison is available yet.

## Quick start

Requirements: Blender 4.2+ and Nix on x86_64-linux.

1. Build the bridge and add-on bundle:

```sh
nix build .#blender-compact-mcp
nix build .#addon
```

2. In Blender, open **Edit → Preferences → Add-ons → Install from Disk**, choose
   `result/compact_blender-0.2.0.zip`, and enable
   **Compact Blender MCP**.
3. In the 3D Viewport sidebar (`N`), open **Compact MCP**. Review permissions and click **Start bridge**.
   All five permissions default on. Stop/restart to change them.
4. Add the absolute path to the Nix-built `blender-compact-mcp` command to your MCP client:

```json
{
  "mcpServers": {
    "blender-compact": {
      "command": "/nix/store/.../bin/blender-compact-mcp"
    }
  }
}
```

Use the client's own MCP configuration format; the block above illustrates a common JSON form.
No token needs to be pasted into model context. The bridge reads the local descriptor.

For multiple Blender instances, set `BLENDER_COMPACT_CONNECTION` to the intended descriptor:
Windows `%LOCALAPPDATA%/blender-compact-mcp/connection-<pid>.json`;
Linux/macOS `${XDG_STATE_HOME:-~/.local/state}/blender-compact-mcp/connection-<pid>.json`.
Ambiguous discovery fails rather than picking an arbitrary scene. After a crash, remove only the
stale instance's descriptor. Normal shutdown removes it; opening a different file stops the server.

## Try it

First ask the agent to discover `primitive`, `array`, `material`, and `assign_material`, then:

> Create a teal cube named Demo at [0,0,1], then make 49 linked copies along X. Inspect the
> first and last object only. Do not render until I ask.

Or use the CLI:

```sh
blender-compact discover
blender-compact inspect
blender-compact execute --params '{"steps":[{"op":"primitive","kind":"cube","name":"Demo","location":[0,0,1]},{"op":"array","name":"Demo","count":49,"offset":[2.5,0,0],"prefix":"Copy"}]}'
```

For PowerShell, avoid nested JSON quoting by piping a file:

```powershell
Get-Content -Raw examples/scene.json | blender-compact execute --params -
```

That example creates 50 cubes, a floor, camera and lights in one batch. Names must be unique;
it intentionally refuses to overwrite existing objects. Run it in a fresh scene.

```sh
blender-compact capture --params '{"filename":"preview.png","size":768}'
```

The CLI returns an output filename; MCP returns an image. Exports go under the local state
directory's `exports/` folder. A repeated filename is refused. Capture does not include other apps
or take a desktop screenshot. Save exports a **copy**, never overwrites a file, and requires save permission.

## Measured overhead, not marketing

In a real Blender test, the same 50 transforms produced identical final object properties:

| Protocol payload | 50 individual calls | One batch |
|---|---:|---:|
| Argument tokens | 1,149 | 1,002 |
| Result tokens | 800 | 16 |
| Total | 1,949 | 1,018 |

**47.77% fewer payload tokens**, using `cl100k_base`. Results are measured from actual requests and
responses, not guessed from characters. This isolates batching using our own bridge in both cases.
It excludes model reasoning, conversation history, caching, tool-call envelopes, images, setup and
discovery. It is **not a benchmark against another product and not a claim of 47.77% lower total bills**.
The agent's quality, task and client-side tool loading still matter. See [validation](docs/validation.md).

## Architecture

```text
AI client -- MCP stdio --> Python SDK bridge -- authenticated loopback JSON --> Blender add-on
CLI ----------------------------------------------^                        bpy.app.timers
```

Blender's timer accepts bounded socket input and executes operations on its main thread. No Python
background thread accesses Blender data. The optional Python operation has full local process privileges. `discover` returns
human-readable argument contracts; operations are validated again by the add-on before execution.
See [design](docs/design.md) and [security](SECURITY.md).

## Development and tests

```sh
nix flake check
nix build .#blender-compact-mcp
nix build .#addon
```

Blender-dependent integration tests are skipped by the Nix check because they must launch a separate
factory-startup Blender process. They never connect to an already-open user scene or save user
preferences. Outputs and logs go to ignored `artifacts/`; the shared NixOS setup validates the live
bridge separately with `blender-compact discover` and `blender-compact inspect`.

The `blender-compact-mcp` package contains the MCP bridge/CLI and the add-on source. The separate
`addon` output contains the installable Blender ZIP and its SHA256 checksum. This fork follows
commit-pinned main snapshots rather than publishing PyPI packages or formal GitHub releases.

MIT licensed. Independent software, not affiliated with Blender or any model provider.
