#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Reject altered source-bound SPI DMA and synthetic QEMU build receipts."""
import argparse
import json
from pathlib import Path
import tempfile

import ws53_ci_qemu_uart_clock_build as policy
import ws53_spi_dma_model_build as builder


def expect_rejected(receipt, changes, validate):
    original = json.loads(receipt.read_text())
    rejected = []
    for label, field, value in changes:
        modified = dict(original)
        modified[field] = value
        # Both validators deliberately bind paths to the receipt directory.
        # Preserve that directory, changing only one copied receipt field.
        with tempfile.NamedTemporaryFile(mode='w', dir=receipt.parent, suffix='.json') as copy:
            json.dump(modified, copy)
            copy.flush()
            try:
                validate(Path(copy.name))
            except ValueError:
                rejected.append(label)
            else:
                raise ValueError(f'accepted altered receipt: {label}')
    return rejected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--width', choices=('8', '32'), required=True)
    args = parser.parse_args()
    builder.configure(args.width)
    builder.validate(args.receipt, args.elf)
    policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    rejected = expect_rejected(args.receipt, [
        ('firmware-source', 'sources', {}),
        ('firmware-helper', 'helpers', {}),
        ('firmware-command', 'command', []),
        ('firmware-binary', 'elf_sha256', '0'*64),
        ('firmware-build-log', 'build_log_sha256', '0'*64),
    ], lambda receipt: builder.validate(receipt, args.elf))
    rejected += expect_rejected(args.model_receipt, [
        ('model-policy', 'policy_sha256', '0'*64),
        ('model-source', 'source_before', {}),
        ('model-binary', 'binary_sha256', '0'*64),
        ('model-qualification', 'hardware_qualification_eligible', True),
    ], lambda receipt: policy.validate_receipt(receipt, args.qemu, source_root=args.model_source))
    # Check originals a second time; no test is allowed to alter a build.
    builder.validate(args.receipt, args.elf)
    policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    print(json.dumps(dict(passed=True, width=int(args.width), rejected=rejected, hardware_accessed=False)))


if __name__ == '__main__':
    main()
