{ pkgs, src }:

pkgs.python3Packages.buildPythonApplication rec {
  pname = "blender-compact-mcp";
  version = "0.2.0";
  pyproject = true;

  inherit src;

  nativeBuildInputs = [ pkgs.python3Packages.hatchling ];
  dependencies = [ pkgs.python3Packages.mcp ];

  postInstall = ''
    mkdir -p "$out/share/blender-addons"
    cp -r addon/compact_blender "$out/share/blender-addons/"
  '';

  pythonImportsCheck = [
    "compact_mcp.cli"
    "compact_mcp.server"
  ];

  meta = {
    description = "Four-tool Blender MCP bridge with batched operations";
    homepage = "https://github.com/KacperSzw/blender-astra-mcp";
    license = pkgs.lib.licenses.mit;
    mainProgram = "blender-compact-mcp";
  };
}
