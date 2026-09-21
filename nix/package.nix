{ pkgs, src }:
let
  projectSource = src;
in
pkgs.rustPlatform.buildRustPackage rec {
  pname = "blender-compact-mcp";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package.version;

  src = pkgs.lib.fileset.toSource {
    root = projectSource;
    fileset = pkgs.lib.fileset.unions [
      (projectSource + "/Cargo.toml")
      (projectSource + "/Cargo.lock")
      (pkgs.lib.fileset.fileFilter (file: file.hasExt "rs") (projectSource + "/src"))
      (pkgs.lib.fileset.fileFilter (file: file.hasExt "py") (projectSource + "/addon"))
      (pkgs.lib.fileset.fileFilter (file: file.hasExt "rs" || file.hasExt "json") (
        projectSource + "/tests"
      ))
      (pkgs.lib.fileset.fileFilter (file: file.hasExt "py") (projectSource + "/scripts"))
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  nativeCheckInputs = [
    pkgs.blender
    pkgs.rustfmt
    pkgs.clippy
  ];
  preCheck = ''
    export BLENDER_EXE="${pkgs.blender}/bin/blender"
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings
  '';

  postInstall = ''
    mkdir -p "$out/share/blender-addons"
    cp -r addon/compact_blender "$out/share/blender-addons/"
  '';

  meta = {
    description = "Four-tool Blender MCP bridge with batched operations";
    homepage = "https://github.com/KacperSzw/blender-astra-mcp";
    license = pkgs.lib.licenses.mit;
    mainProgram = "blender-compact-mcp";
  };
}
