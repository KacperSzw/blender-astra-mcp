"""Blender operations. Called ONLY on Blender's main thread."""

import math
import re
from fnmatch import fnmatchcase
from pathlib import Path

import bpy
from mathutils import Vector

from .catalog import CATALOG, PERMISSIONS, discover
from .snippets import OutputBudget, Snippets, run_python

OWNER = "blender_compact_mcp"


def number(value, lo=-100000, hi=100000):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError("Expected finite number")
    if not math.isfinite(value) or not lo <= value <= hi:
        raise ValueError(f"Number must be finite and between {lo} and {hi}")
    return float(value)


def vector(value, length=3):
    if not isinstance(value, list) or len(value) != length:
        raise ValueError(f"Expected {length}-element numeric list")
    return [number(v) for v in value]


def name(value):
    if not isinstance(value, str) or not value or "\x00" in value or len(value.encode("utf-8")) > 255:
        raise ValueError("Name must be nonempty UTF-8 text, at most 255 bytes, without NUL")
    return value


def owned(value):
    obj = bpy.data.objects.get(name(value))
    if obj is None:
        raise ValueError(f"Object not found: {value}")
    if obj.name not in bpy.context.scene.objects:
        raise ValueError("Object is outside the active scene")
    return obj


def output_file(root, filename, suffix):
    if not isinstance(filename, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,100}", filename):
        raise ValueError("Output must be a simple filename, not a path")
    if not filename.endswith(suffix):
        raise ValueError(f"Output must end in {suffix}")
    root = Path(root).resolve()
    path = root / filename
    if path.exists() or path.is_symlink():
        raise ValueError("Refusing to overwrite an existing output")
    root.mkdir(parents=True, exist_ok=True)
    return path


def validate_step(step):
    if not isinstance(step, dict):
        raise ValueError("Each step must be an object")
    op = step.get("op")
    if not isinstance(op, str) or op not in CATALOG:
        raise ValueError(f"Unknown operation: {op}; discover lists operations")
    extra = set(step) - {"op"} - set(CATALOG[op]["args"])
    if extra:
        raise ValueError(
            f"Unknown arguments for {op}: {sorted(extra)}; allowed: {','.join(CATALOG[op]['args'])}"
        )
    for key in CATALOG[op]["required"]:
        if key not in step:
            raise ValueError(f"{op} needs {key}")
    for key in ("name", "prefix", "material", "modifier", "data_path"):
        if key in step:
            name(step[key])
    from .extended import validate

    if validate(op, step):
        return
    for key in ("location", "rotation", "scale", "offset", "target"):
        if key in step:
            vector(step[key])
    if "color" in step:
        for v in vector(step["color"], 4):
            number(v, 0, 1)
    for key, lo, hi in (
        ("metallic", 0, 1),
        ("roughness", 0, 1),
        ("energy", 0, 10000),
        ("size", 0.01, 100),
        ("lens", 10, 200),
    ):
        if key in step:
            number(step[key], lo, hi)
    if op == "primitive" and step["kind"] not in ("cube", "sphere", "plane", "cylinder", "cone"):
        raise ValueError("Unknown primitive kind")
    if op == "light" and step.get("kind", "AREA") not in ("AREA", "POINT", "SUN"):
        raise ValueError("Unknown light kind")
    if op == "array" and (type(step["count"]) is not int or not 1 <= step["count"] <= 200):
        raise ValueError("count must be an integer from 1 to 200")
    if op == "delete":
        if step["confirm"] is not True:
            raise ValueError("Deletion needs confirm=true")
        if not isinstance(step["names"], list) or not 1 <= len(step["names"]) <= 200:
            raise ValueError("names must contain 1..200 names")
        for v in step["names"]:
            name(v)


class Engine:
    def __init__(self, output_dir, permissions=("write", "render")):
        self.output_dir = Path(output_dir)
        self.permissions = set(permissions)
        self.snippets = Snippets()

    def collection(self):
        scene = bpy.context.scene
        for col in scene.collection.children:
            if col.get(OWNER) is True:
                return col
        col = bpy.data.collections.new("Compact MCP")
        col[OWNER] = True
        scene.collection.children.link(col)
        return col

    def attach(self, obj):
        obj[OWNER] = True
        for col in list(obj.users_collection):
            col.objects.unlink(obj)
        self.collection().objects.link(obj)
        return obj

    def inspect(self, names=None, limit=20, offset=0, match=None, fields=None, context=False):
        # Rust projects optional context into MCP text; CLI JSON keeps its metadata.
        if type(context) is not bool:
            raise ValueError("context must be boolean")
        if type(limit) is not int or not 1 <= limit <= 100 or type(offset) is not int or offset < 0:
            raise ValueError("limit must be 1..100 and offset nonnegative")
        if names is not None and (not isinstance(names, list) or len(names) > 100):
            raise ValueError("names must be a list of at most 100 names")
        if names is not None:
            for value in names:
                name(value)
        if names is not None and match is not None:
            raise ValueError("names and match are mutually exclusive")
        if match is not None:
            name(match)
        available = (
            "name",
            "type",
            "managed",
            "location",
            "scale",
            "rotation",
            "vertices",
            "polygons",
            "materials",
        )
        if fields is not None and (
            not isinstance(fields, list)
            or len(fields) > len(available)
            or any(not isinstance(f, str) or f not in available for f in fields)
        ):
            raise ValueError(f"fields must select from {','.join(available)}")
        selected = set(fields if fields is not None else available if names is not None else available[:4])
        scene = bpy.context.scene
        candidates = (
            (scene.objects.get(n) for n in dict.fromkeys(names)) if names is not None else scene.objects
        )
        objects = sorted(
            (o for o in candidates if o is not None and (match is None or fnmatchcase(o.name, match))),
            key=lambda o: o.name,
        )
        rows = []
        for obj in objects[offset : offset + limit]:
            row = {"name": obj.name}
            if "type" in selected:
                row["type"] = obj.type
            if "managed" in selected:
                row["managed"] = obj.get(OWNER) is True
            for field, attribute in (
                ("location", "location"),
                ("scale", "scale"),
                ("rotation", "rotation_euler"),
            ):
                if field in selected:
                    row[field] = [round(v, 4) for v in getattr(obj, attribute)]
            if obj.type == "MESH":
                if "vertices" in selected:
                    row["vertices"] = len(obj.data.vertices)
                if "polygons" in selected:
                    row["polygons"] = len(obj.data.polygons)
                if "materials" in selected:
                    row["materials"] = [m.name if m else None for m in obj.data.materials]
            rows.append(row)
        result = {
            "blender": bpy.app.version_string,
            "scene": scene.name,
            "permissions": sorted(self.permissions),
            "total": len(objects),
            "objects": rows,
            "next_offset": offset + limit if offset + limit < len(objects) else None,
        }
        if names is not None:
            missing = [n for n in dict.fromkeys(names) if scene.objects.get(n) is None]
            if missing:
                result["missing"] = missing
        return result

    def execute(self, steps, dry_run=False):
        if not isinstance(steps, list) or not 1 <= len(steps) <= 100:
            raise ValueError("steps must contain 1..100 operations")
        if type(dry_run) is not bool:
            raise ValueError("dry_run must be boolean")
        # Validate syntax and permissions for the entire batch before any writes.
        # Existence checks remain per-operation to permit references to earlier creations.
        prepared = []
        for step in steps:
            validate_step(step)
            if PERMISSIONS[step["op"]] is not None and PERMISSIONS[step["op"]] not in self.permissions:
                raise PermissionError(f"Disabled permission: {PERMISSIONS[step['op']]}")
            prepared.append(self.snippets.prepare(step) if step["op"] == "python" else None)
        if sum(s.get("count", 1) for s in steps) > 500:
            raise ValueError("Batch expands to more than 500 operations")
        if bpy.context.mode != "OBJECT" and any(s["op"] not in ("python", "frame", "rna") for s in steps):
            raise ValueError("Switch Blender to Object Mode first")
        if dry_run:
            return {"validated": len(steps), "executed": 0, "scope": "syntax and permissions only"}
        # Blender's interactive undo stack is context-dependent. Record checkpoints where
        # available; background mode deliberately makes no undo promise. Files are never undone.
        can_undo = not bpy.app.background and bpy.ops.ed.undo_push.poll()
        if can_undo:
            bpy.ops.ed.undo_push(message="Before Compact MCP batch")
        try:
            return self.run_steps(steps, prepared)
        finally:
            if can_undo:
                bpy.ops.ed.undo_push(message="Compact MCP batch")

    def run_steps(self, steps, prepared):
        response = {"ok": True, "completed": 0, "changed": 0, "files": []}
        values = []
        budget = OutputBudget()
        for index, step in enumerate(steps):
            try:
                if step["op"] == "python":
                    code, handle = self.snippets.activate(prepared[index])
                    item = {"index": index, "result": {"script": handle}}
                    values.append(item)
                    response["changed"] = None
                    payload, error = run_python(self, step, code, budget)
                    item["result"].update(payload)
                    if error is not None:
                        raise RuntimeError(error)
                    response["completed"] += 1
                    continue
                result = self.step(step)
                if response["changed"] is not None:
                    response["changed"] += result.get("changed", 0)
                if "file" in result:
                    response["files"].append(result)
                if "result" in result:
                    values.append({"index": index, "result": result["result"]})
                response["completed"] += 1
            except Exception as exc:
                response.update(
                    ok=False,
                    failed_index=index,
                    error=str(exc)[:2048],
                    atomic=False,
                    note="Earlier operations persist; failed operation may have partial effects.",
                )
                break
        if values:
            response["results"] = values
        return response

    def step(self, s):
        op = s["op"]
        from .extended import run

        result = run(self, s)
        if result is not None:
            return result
        if op in ("primitive", "light", "camera") and s["name"] in bpy.data.objects:
            raise ValueError(f"Object already exists: {s['name']}")
        if op == "primitive":
            operators = {
                "cube": bpy.ops.mesh.primitive_cube_add,
                "sphere": bpy.ops.mesh.primitive_uv_sphere_add,
                "plane": bpy.ops.mesh.primitive_plane_add,
                "cylinder": bpy.ops.mesh.primitive_cylinder_add,
                "cone": bpy.ops.mesh.primitive_cone_add,
            }
            operators[s["kind"]](location=s.get("location", [0, 0, 0]))
            obj = self.attach(bpy.context.object)
            obj.name = s["name"]
            obj.scale = s.get("scale", [1, 1, 1])
        elif op == "transform":
            obj = owned(s["name"])
            for key, attr in (("location", "location"), ("rotation", "rotation_euler"), ("scale", "scale")):
                if key in s:
                    setattr(obj, attr, s[key])
        elif op == "material":
            mat = bpy.data.materials.get(s["name"])
            mat = mat or bpy.data.materials.new(s["name"])
            mat[OWNER] = True
            mat.use_nodes = True
            node = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
            node.inputs["Base Color"].default_value = s["color"]
            node.inputs["Metallic"].default_value = s.get("metallic", 0)
            node.inputs["Roughness"].default_value = s.get("roughness", 0.5)
            mat.diffuse_color = s["color"]
        elif op == "assign_material":
            obj = owned(s["name"])
            mat = bpy.data.materials.get(s["material"])
            if obj.type != "MESH" or mat is None:
                raise ValueError("Need mesh and existing material")
            # Array copies share mesh data. Detach before changing material slots.
            if obj.data.users > 1:
                obj.data = obj.data.copy()
            obj.data.materials.clear()
            obj.data.materials.append(mat)
        elif op == "array":
            obj = owned(s["name"])
            names = [f"{s['prefix']}_{i:03d}" for i in range(1, s["count"] + 1)]
            if any(len(n) > 63 or n in bpy.data.objects for n in names):
                raise ValueError("Array copy names collide or exceed 63 characters")
            for i, n in enumerate(names, 1):
                copy = obj.copy()
                copy.name = n
                copy.location = obj.location + Vector(s["offset"]) * i
                self.attach(copy)
            return {"changed": len(names)}
        elif op in ("light", "camera"):
            data = (
                bpy.data.lights.new(s["name"], s.get("kind", "AREA"))
                if op == "light"
                else bpy.data.cameras.new(s["name"])
            )
            obj = self.attach(bpy.data.objects.new(s["name"], data))
            obj.location = s["location"]
            if "target" in s:
                direction = Vector(s["target"]) - obj.location
                if direction.length > 0:
                    obj.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
            if op == "camera":
                data.lens = s.get("lens", 50)
                bpy.context.scene.camera = obj
            else:
                data.energy = s["energy"]
                if data.type == "AREA":
                    data.size = s.get("size", 5)
        elif op == "delete":
            objects = [owned(n) for n in dict.fromkeys(s["names"])]
            for obj in objects:
                bpy.data.objects.remove(obj, do_unlink=True)
            return {"changed": len(objects)}
        elif op == "save":
            path = output_file(self.output_dir, s["filename"], ".blend")
            bpy.ops.wm.save_as_mainfile(filepath=str(path), copy=True)
            return {"changed": 0, "file": path.name}
        return {"changed": 1}

    def capture(self, filename="preview.png", size=512, view="camera"):
        if "render" not in self.permissions:
            raise PermissionError("Disabled permission: render")
        if type(size) is not int or not 64 <= size <= 1024:
            raise ValueError("size must be an integer from 64 to 1024")
        scene = bpy.context.scene
        if view not in ("camera", "viewport"):
            raise ValueError("view must be camera or viewport")
        if view == "viewport" and bpy.app.background:
            raise ValueError("Viewport capture requires interactive Blender")
        if view == "camera" and scene.camera is None:
            raise ValueError("Scene has no active camera")
        path = output_file(self.output_dir, filename, ".png")
        render = scene.render
        attrs = (
            "filepath",
            "resolution_x",
            "resolution_y",
            "resolution_percentage",
            "engine",
            "use_file_extension",
        )
        old = {a: getattr(render, a) for a in attrs}
        old_format = render.image_settings.file_format
        try:
            render.engine = "CYCLES"
            samples = scene.cycles.samples
            scene.cycles.samples = 16
            render.filepath = str(path)
            render.resolution_x = render.resolution_y = size
            render.resolution_percentage = 100
            render.image_settings.file_format = "PNG"
            render.use_file_extension = True
            if view == "camera":
                bpy.ops.render.render(write_still=True)
            else:
                area = next((a for a in bpy.context.screen.areas if a.type == "VIEW_3D"), None)
                if area is None:
                    raise ValueError("Open a 3D Viewport first")
                region = next(r for r in area.regions if r.type == "WINDOW")
                with bpy.context.temp_override(area=area, region=region):
                    bpy.ops.render.opengl(write_still=True, view_context=True)
            if not path.is_file():
                raise RuntimeError("Render did not produce a PNG")
            return {"file": path.name, "width": size, "height": size, "bytes": path.stat().st_size}
        finally:
            if "samples" in locals():
                scene.cycles.samples = samples
            for attr, value in old.items():
                setattr(render, attr, value)
            render.image_settings.file_format = old_format

    def dispatch(self, method, params):
        if method == "discover":
            return discover(**params)
        if method == "inspect":
            return self.inspect(**params)
        if method == "execute":
            extra = set(params) - {"code", "script", "params", "max_output", "steps", "dry_run"}
            if extra:
                raise ValueError(f"Unknown execute arguments: {sorted(extra)}")
            if sum(key in params for key in ("code", "script", "steps")) != 1:
                raise ValueError("Exactly one of code, script, steps is required")
            if "steps" in params:
                if "params" in params or "max_output" in params:
                    raise ValueError("params/max_output belong inside each Python step when using steps")
            else:
                params = params.copy()
                step = {"op": "python"}
                for key in ("code", "script", "params", "max_output"):
                    if key in params:
                        step[key] = params.pop(key)
                params["steps"] = [step]
            return self.execute(**params)
        if method == "capture":
            return self.capture(**params)
        raise ValueError("Unknown method")
