# v0.4 tool inputs and outputs

MCP arguments are JSON. Responses below are the single text content block; the CLI returns
underlying JSON with completion/change counters. Handles are session-specific: copy the one
returned by your own call. These examples assume the named objects exist.

## Edit without returned data

`execute`:

```json
{"steps":[{"op":"transform","name":"BenchSeed","location":[1,2,3]}]}
```

```text
ok
```

## Direct Python and reuse

`execute` source from the measured snippet task:

```json
{"code":"for name in params[\"names\"]:\n    obj=bpy.data.objects[name]\n    obj.location.z=params[\"z\"]\n    obj.hide_render=False\n    obj[\"benchmark\"]=True\nresult={\"updated\":len(params[\"names\"])}","params":{"names":["BenchSeed","Bench_049"],"z":1}}
```

```text
ok
script=8d43c2f2d3f7ee7f:1
result: {"updated":2}
```

`execute` the returned handle with new parameters:

```json
{"script":"8d43c2f2d3f7ee7f:1","params":{"names":["BenchSeed","Bench_049"],"z":2}}
```

```text
ok
result: {"updated":2}
```

No discovery is needed for direct Python. Omit `result` when no data is needed; then source
execution returns `ok` plus a new handle, and reuse returns just `ok`. Assigning `result=None`
intentionally returns `result: null`. Module-level `return` is invalid. Printed text appears
as `stdout: "..."`, with explicit truncation labels when necessary.

## Uniform records

For a script assigning a list such as
`result=[{"name":"Cube","vertices":8},{"name":"Plane","vertices":4}]`, the payload after
the new-source handle is:

```text
result rows=2:
name	vertices
Cube	8
Plane	4
```

Columns use actual tabs. Nested or irregular records remain compact JSON. Strings such as
`"001"`, `"false"`, and names containing whitespace are JSON-quoted; numbers, booleans, and
null keep their types. Integer digits are preserved even beyond 64 bits. Long tables stop
at complete rows and show an output-truncated marker. Filter or aggregate inside the script
when the full data is unnecessary; the script's `max_output` budget applies before formatting.

## Targeted inspection

`inspect`, from the measured scene after its transform task:

```json
{"names":["BenchSeed","Bench_049"],"fields":["location"]}
```

```text
scene=Scene total=2
name	location
BenchSeed	[0,3,1]
Bench_049	[49,3,1]
```

Add `context:true` for Blender version and enabled permissions. When `next_offset` appears,
repeat the same filters/fields with that offset, even if fewer than `limit` rows were returned.
Keep the scene unchanged while paging; stop when `next_offset` is absent. `-` denotes an
inapplicable/missing cell, distinct from `null`. Missing requested object names are listed separately.

## Failure after a completed step

`execute`:

```json
{"steps":[{"op":"transform","name":"BenchSeed","location":[1,2,3]},{"op":"transform","name":"Missing","location":[0,0,0]}]}
```

```text
error failed_index=1 completed=1 partial=true
Object not found: Missing
```

The MCP result has `isError:true`. Earlier changes remain; a failing step can also have partial
effects. Available earlier values, file names, and new handles follow the error with step labels
where needed. Inspect before retrying. A timeout also does not cancel execution.

## Validation only

`execute`:

```json
{"code":"bpy.context.scene.frame_set(18)","dry_run":true}
```

```text
validated=1 executed=0 scope=syntax/permissions
```

No code ran and no handle was allocated. Runtime context, object existence, and filesystem
success are not guaranteed. `code`, `script`, and `steps` cannot be supplied together.
