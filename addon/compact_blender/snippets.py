"""Session-local compiled snippets and bounded script output. Blender thread only."""

import contextlib
import io
import json
import secrets
from collections import OrderedDict

MAX_SNIPPETS = 64
MAX_SOURCE_BYTES = 1024 * 1024
MAX_BATCH_OUTPUT = 32 * 1024


class Snippets:
    def __init__(self):
        self.session = secrets.token_hex(8)
        self.counter = 0
        self.entries = OrderedDict()
        self.source_bytes = 0

    def prepare(self, step):
        if "script" in step:
            handle = step["script"]
            if handle not in self.entries:
                raise ValueError("Unknown/expired script; submit code again only if execution is intended")
            code, size = self.entries[handle]
            return code, size, handle
        source = step["code"]
        size = len(source.encode("utf-8"))
        if size > MAX_SOURCE_BYTES:
            raise ValueError("Script exceeds the session source budget")
        try:
            code = compile(source, "<compact-script>", "exec")
        except SyntaxError as exc:
            if exc.msg == "'return' outside function":
                raise ValueError("Module-level Python: assign result instead of top-level return") from None
            raise
        return code, size, None

    def activate(self, prepared):
        code, size, handle = prepared
        if handle is None:
            self.counter += 1
            handle = f"{self.session}:{self.counter}"
            while len(self.entries) >= MAX_SNIPPETS or self.source_bytes + size > MAX_SOURCE_BYTES:
                _, (_, old_size) = self.entries.popitem(last=False)
                self.source_bytes -= old_size
            self.entries[handle] = (code, size)
            self.source_bytes += size
        elif handle in self.entries:
            self.entries.move_to_end(handle)
        return code, handle


class OutputBudget:
    def __init__(self):
        self.remaining = MAX_BATCH_OUTPUT

    def take(self, text, characters):
        part = text[:characters].encode("utf-8")[: self.remaining].decode("utf-8", errors="ignore")
        self.remaining -= len(part.encode("utf-8"))
        return part


class ScriptOutput(io.TextIOBase):
    def __init__(self, budget, limit):
        self.budget = budget
        self.remaining = limit
        self.parts = []
        self.total = 0

    def take(self, text):
        part = self.budget.take(text, self.remaining)
        self.remaining -= len(part)
        return part

    def write(self, text):
        self.total += len(text)
        part = self.take(text)
        if part:
            self.parts.append(part)
        return len(text)


def run_python(engine, step, code, budget):
    import bpy

    capture = ScriptOutput(budget, step.get("max_output", 2000))
    namespace = {"bpy": bpy, "params": step.get("params", {}), "output_dir": str(engine.output_dir)}
    result = {}
    error = None
    try:
        with contextlib.redirect_stdout(capture), contextlib.redirect_stderr(capture):
            exec(code, namespace)
        if "result" in namespace:
            parts = []
            total = 0
            # Validate the entire value, without building an unbounded serialized copy.
            for chunk in json.JSONEncoder(
                ensure_ascii=False, allow_nan=False, separators=(",", ":")
            ).iterencode(namespace["result"]):
                total += len(chunk)
                part = capture.take(chunk)
                if part:
                    parts.append(part)
            preview = "".join(parts)
            if total > len(preview):
                result.update(result_preview=preview, result_truncated=True, result_chars=total)
            else:
                result["value"] = json.loads(preview)
    except Exception as exc:
        error = f"{type(exc).__name__}: {exc}"
    if capture.total:
        stdout = "".join(capture.parts)
        result.update(stdout=stdout, stdout_truncated=capture.total > len(stdout))
    return result, error
