# Security

This is a local editor-control service for a trusted workstation, not an OS sandbox.

- The add-on binds to loopback and authenticates every request before dispatch. A new random secret
  is generated at each bridge start. Rust rejects non-loopback or incompatible descriptors.
- Descriptors are credentials. They are published atomically in the user's application-state
  directory; Unix files use mode 0600. Windows relies on the profile directory's inherited ACLs.
  Same-user processes can read them. Never sync them, paste their tokens into prompts, or commit them.
- Blender enforces write, render, delete, save, and Python permissions, fixed at bridge start.
  All default on in this workstation fork. Clients cannot grant themselves permission.
- **Python is unrestricted local code execution.** It can access files/network/processes and bypass
  the narrower toggles. Cached snippets require Python permission every time; caching does not sandbox code.
- Existing active-scene objects are editable. Managed tags indicate provenance only. Shared material
  changes and object deletion can affect other scenes using the same data.
- Delete additionally requires `confirm: true`. This is an intent flag, not proof of human approval.
- Save/capture accept simple non-overwriting output filenames. Rust restricts PNG reads to the
  descriptor's export directory and rejects symlinks. This is not protection from malicious same-user
  filesystem races or unrestricted Python code.
- Scene names, selected properties, script results, and captures may be sent to the model provider.
  Output bounds reduce accidental context growth; they are not a data-access policy.
- Batches are not transactions. Files and partial writes persist after failures. Timeout/disconnect
  does not cancel an operation; the client reports uncertainty and never automatically replays mutations.
- Blender operations run synchronously on its main thread. Arbitrary scripts, renders, and large
  serialization workloads can block the UI. Bounded retained output does not limit script CPU/memory use.
- The project has no telemetry. Runtime descriptors, exports, and logs are excluded from commits.
  Redact credentials and proprietary scene data when reporting issues.

Tests launch factory-startup Blender with isolated configuration and outputs. The optional GUI test
uses the caller's selected native workspace without requesting focus and closes only its own instance.
