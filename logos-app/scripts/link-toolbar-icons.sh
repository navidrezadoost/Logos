#!/usr/bin/env bash
# Copy Font Awesome sharp-solid toolbar icons (fallback: mcp/icons/solid).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$ROOT/.." && pwd)"
DEFAULT_FA="${LOGOS_FA_SHARP_DIR:-$HOME/Documents/FontAwesome.Pro.7.1.0.0/fontawesome-pro-7.1.0-web/svgs-full/sharp-solid}"
ICONS="${LOGOS_ICONS_DIR:-$DEFAULT_FA}"
if [[ ! -d "$ICONS" ]]; then
  ICONS="$REPO/mcp/icons/solid"
fi
DEST="$ROOT/src/icons/toolbar"

if [[ ! -d "$ICONS" ]]; then
  echo "Icon source not found: $ICONS" >&2
  echo "Set LOGOS_ICONS_DIR to your solid icons directory." >&2
  exit 1
fi

mkdir -p "$DEST"
declare -A MAP=(
  [select]=arrow-pointer
  [hand]=hand
  [scale]=expand
  [frame]=frame
  [selection]=crop
  [slice]=scissors
  [rect]=rectangle
  [line]=horizontal-rule
  [arrow]=arrow-right
  [ellipse]=circle
  [polygon]=hexagon
  [star]=star
  [imageImport]=image
  [text]=font
  [path]=pen-ruler
  [prototype]=diagram-project
  [dev]=code
  [boolUnion]=object-union
  [boolIntersect]=object-intersect
  [boolSubtract]=object-subtract
  [boolExclude]=object-exclude
  [resetView]=magnifying-glass-arrows-rotate
  [chevronDown]=chevron-down
)

for name in "${!MAP[@]}"; do
  src="$ICONS/${MAP[$name]}.svg"
  if [[ ! -f "$src" ]]; then
    echo "Missing icon: $src" >&2
    exit 1
  fi
  cp "$src" "$DEST/${name}.svg"
done

echo "Copied ${#MAP[@]} toolbar icons → $DEST"
