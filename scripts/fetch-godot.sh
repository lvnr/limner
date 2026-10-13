#!/usr/bin/env bash
# Downloads the pinned Godot editor build into tools/godot/<version>/ and verifies its checksum.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(cat "$ROOT/scripts/godot-version.txt")"
BASE="https://github.com/godotengine/godot-builds/releases/download/$VERSION"
case "$(uname -s)" in
  Darwin) ASSET="Godot_v${VERSION}_macos.universal.zip" ;;
  Linux)  ASSET="Godot_v${VERSION}_linux.x86_64.zip" ;;
  *) echo "Unsupported platform; on Windows use scripts/fetch-godot.ps1" >&2; exit 1 ;;
esac
DEST="$ROOT/tools/godot/$VERSION"
DL="$ROOT/tools/godot/dl"
mkdir -p "$DEST" "$DL"
curl -sSL -o "$DL/SHA512-SUMS.txt" "$BASE/SHA512-SUMS.txt"
curl -sSL -o "$DL/$ASSET" "$BASE/$ASSET"
(cd "$DL" && grep " $ASSET\$" SHA512-SUMS.txt | shasum -a 512 -c -)
unzip -q -o "$DL/$ASSET" -d "$DEST"
"$ROOT/scripts/godot.sh" --version
