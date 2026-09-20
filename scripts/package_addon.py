"""Build an installable Blender add-on ZIP, excluding caches and runtime state."""

import hashlib
import tomllib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
dist = root / "dist"
dist.mkdir(exist_ok=True)
target = dist / f"compact_blender-{version}.zip"
with ZipFile(target, "w", ZIP_DEFLATED) as archive:
    for path in sorted((root / "addon/compact_blender").glob("*.py")):
        archive.write(path, f"compact_blender/{path.name}")
(dist / "SHA256SUMS").write_text(f"{hashlib.sha256(target.read_bytes()).hexdigest()}  {target.name}\n")
print(target)
