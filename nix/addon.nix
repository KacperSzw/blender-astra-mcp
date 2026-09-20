{
  pkgs,
  src,
  version,
}:

pkgs.runCommand "compact-blender-addon-${version}"
  {
    nativeBuildInputs = [ pkgs.zip ];
  }
  ''
    mkdir -p "$out" staging/compact_blender
    cp ${src}/addon/compact_blender/*.py staging/compact_blender/
    (cd staging && zip -q -r "$out/compact_blender-${version}.zip" compact_blender)
    sha256sum "$out/compact_blender-${version}.zip" > "$out/SHA256SUMS"
  ''
