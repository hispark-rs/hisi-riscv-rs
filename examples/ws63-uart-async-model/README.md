# WS63 async UART RX model fixture

Model only: UART0 carries diagnostics, UART1 receives from the runner's socket
peer. It exercises the public async Read trait and actual IRQ54, retained burst
bytes, empty/duplicate notifications, empty read, cancellation and rearming.
Manual Future polling isolates the known independent `block_on` idle problem.
This is diagnostic fixture code, not a second executor or product API.

Build this isolated workspace, then run `scripts/test-ws63-uart-async-qemu.py`
with its ELF, a QEMU implementing the corrected v151 status/IRQ fields, and a
new output directory. `--capacity 1` preserves the old synthetic capacity;
`--capacity 16` tests coalesced buffering. Neither establishes physical FIFO depth.
The guest does not use the unqualified receive-count alias or fake completion
MMIO. No board image, serial device or flashing procedure is part of this test.
