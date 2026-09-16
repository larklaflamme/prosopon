#!/bin/bash
# Download the VRoid female VRM (52 ARKit blendshapes) for the avatar renderer.
# The model is gitignored (ui/models/), so fetch it here on each machine.
set -euo pipefail
cd "$(dirname "$0")"

URL="https://raw.githubusercontent.com/hinzka/52blendshapes-for-VRoid-face/main/VRoid_V110_Female_v1.1.3.vrm"
OUT="models/VRoid_V110_Female_v1.1.3.vrm"

mkdir -p models
echo "Downloading VRM from hinzka/52blendshapes-for-VRoid-face ..."
curl -sL -o "$OUT" "$URL"
echo "Done: $(du -h "$OUT" | cut -f1) -> $OUT"
