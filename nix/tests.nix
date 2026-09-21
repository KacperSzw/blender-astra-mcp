{ pkgs, src }:
pkgs.runCommand "blender-compact-addon-style"
  {
    nativeBuildInputs = [ pkgs.ruff ];
  }
  ''
    export RUFF_CACHE_DIR="$TMPDIR/ruff-cache"
    cd ${src}
    ruff check addon scripts
    ruff format --check addon scripts
    touch "$out"
  ''
