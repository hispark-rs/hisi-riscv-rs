#!/usr/bin/env bash
# Integration CI for model-only SFC/SDIO/HASH/KM/SYMC fixtures.
set -euo pipefail

if [[ $# -ne 4 ]]; then
    echo "usage: $0 QEMU MODEL_RECEIPT MODEL_SOURCE NEW_OUTPUT_DIRECTORY" >&2
    exit 2
fi
qemu=$(realpath "$1")
model_receipt=$(realpath "$2")
model_source=$(realpath "$3")
output=$4
if [[ -e "$output" ]]; then
    echo "output already exists: $output" >&2
    exit 2
fi
mkdir -p "$output"
output=$(realpath "$output")

for kind in hash km; do
    uv run --offline --script scripts/ws53_crypto_model_build.py --kind "$kind" --output "$output/$kind-build"
    uv run --offline --script scripts/test-ws53-crypto-qualified.py --kind "$kind" \
        --qemu "$qemu" --model-receipt "$model_receipt" --model-source "$model_source" \
        --elf "$output/$kind-build/firmware.elf" --receipt "$output/$kind-build/receipt.json" \
        --output "$output/$kind-test"
done
for kind in init transfer async; do
    uv run --offline --script scripts/ws53_sdio_model_build.py --kind "$kind" --output "$output/sdio-$kind-build"
    uv run --offline --script scripts/test-ws53-sdio-qualified.py --kind "$kind" \
        --qemu "$qemu" --model-receipt "$model_receipt" --model-source "$model_source" \
        --elf "$output/sdio-$kind-build/firmware.elf" --receipt "$output/sdio-$kind-build/receipt.json" \
        --output "$output/sdio-$kind-test"
done
for kind in symc sfc; do
    uv run --offline --script "scripts/ws53_${kind}_model_build.py" --output "$output/$kind-build"
    uv run --offline --script "scripts/test-ws53-$kind-qualified.py" \
        --qemu "$qemu" --model-receipt "$model_receipt" --model-source "$model_source" \
        --elf "$output/$kind-build/firmware.elf" --receipt "$output/$kind-build/receipt.json" \
        --output "$output/$kind-test"
done
python3 - "$output" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
names = ('hash', 'km', 'sdio-init', 'sdio-transfer', 'sdio-async', 'symc', 'sfc')
rows = {}
for name in names:
    path = root / f'{name}-test/report.json'
    content = path.read_bytes()
    report = json.loads(content)
    if report.get('passed') is not True or report.get('hardware_accessed') is not False:
        raise SystemExit(f'invalid model report: {name}')
    rows[name] = {'cases': report['cases'], 'report_sha256': hashlib.sha256(content).hexdigest()}
(root / 'summary.json').write_text(json.dumps({'passed': True, 'hardware_accessed': False, 'models': rows}, indent=2) + '\n')
print(json.dumps({'passed': True, 'models': rows}))
PY
