{ pkgs, src }:
let
  python = pkgs.python3.withPackages (packages: [
    packages.mcp
    packages.pytest
    packages.ruff
    packages.tiktoken
  ]);
in
pkgs.runCommand "blender-compact-mcp-tests"
  {
    nativeBuildInputs = [ python ];
  }
  ''
    export HOME="$TMPDIR/home"
    export RUFF_CACHE_DIR="$TMPDIR/ruff-cache"
    mkdir -p "$HOME"
    export PYTHONPATH="${src}/src:${src}"
    cd ${src}
    ${python.interpreter} -m ruff check --config pyproject.toml .
    ${python.interpreter} -m ruff format --check --config pyproject.toml .
    ${python.interpreter} -m pytest -q -p no:cacheprovider
    touch "$out"
  ''
