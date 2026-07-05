#!/bin/bash
set -euo pipefail
ELF="$1"
PROBE_RS="${PROBE_RS:-probe-rs}"
CHIP="${HISI_CHIP:-WS63}"

if command -v hisi-fwpkg &>/dev/null; then
    echo "==> patching boot header hash" >&2
    hisi-fwpkg patch-hash "$ELF" >&2
fi

echo "==> flashing $CHIP" >&2
"$PROBE_RS" download --chip "$CHIP" "$ELF" >&2

echo "==> resetting $CHIP" >&2
"$PROBE_RS" reset --chip "$CHIP" >&2

echo "==> done. To debug: press the board reset button, then run:" >&2
echo "    $PROBE_RS attach --chip $CHIP" >&2
