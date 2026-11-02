#!/usr/bin/env bash
# Copy Font Awesome sharp-solid system icons (workspace UI, etc.).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_FA="${LOGOS_FA_SHARP_DIR:-$HOME/Documents/FontAwesome.Pro.7.1.0.0/fontawesome-pro-7.1.0-web/svgs-full/sharp-solid}"
ICONS="${LOGOS_ICONS_DIR:-$DEFAULT_FA}"
DEST="$ROOT/src/icons/system"

if [[ ! -d "$ICONS" ]]; then
  echo "Icon source not found: $ICONS" >&2
  echo "Set LOGOS_FA_SHARP_DIR or LOGOS_ICONS_DIR to sharp-solid SVG directory." >&2
  exit 1
fi

mkdir -p "$DEST"
declare -A MAP=(
  [position]=table-layout
  [positionBottom]=objects-align-bottom
  [positionTop]=objects-align-top
  [positionLeft]=objects-align-left
  [positionRight]=objects-align-right
  [move]=arrow-pointer
  [hand]=hand
  [scale]=expand
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

echo "Copied ${#MAP[@]} system icons → $DEST"
