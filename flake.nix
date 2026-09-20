{
  description = "Nix packaging for the Blender Compact MCP bridge";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/93108a538f079596c9a16c72cf03e9322782b6dd";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      package = import ./nix/package.nix {
        inherit pkgs;
        src = ./.;
      };
      addon = import ./nix/addon.nix {
        inherit pkgs;
        src = ./.;
        version = package.version;
      };
      tests = import ./nix/tests.nix {
        inherit pkgs;
        src = ./.;
      };
    in
    {
      packages.${system} = {
        default = package;
        blender-compact-mcp = package;
        addon = addon;
      };

      checks.${system} = {
        package = package;
        addon = addon;
        tests = tests;
      };

      formatter.${system} = pkgs.nixfmt;
    };
}
