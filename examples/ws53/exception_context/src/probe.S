# First-party TRUE synchronous faults, not writes to mcause/mepc to fake entry.
# Parameter: kind[1:0], FS[5:4], MIE[8]. Keep own ABI/CSR save outside tested frame.
.section .text.ws53_exception_probe, "ax"
.global ws53_exception_probe
.type ws53_exception_probe,@function
ws53_exception_probe:
    addi sp, sp, -448
    sw ra, 288(sp); sw gp, 292(sp); sw tp, 296(sp)
    sw s0, 300(sp); sw s1, 304(sp); sw s2, 308(sp); sw s3, 312(sp)
    sw s4, 316(sp); sw s5, 320(sp); sw s6, 324(sp); sw s7, 328(sp)
    sw s8, 332(sp); sw s9, 336(sp); sw s10, 340(sp); sw s11, 344(sp)
    csrr t0, mstatus; sw t0, 400(sp)
    csrci mstatus, 8
    li t0, 0x6000; csrs mstatus, t0
    fsw f8, 348(sp); fsw f9, 352(sp)
    fsw f18, 356(sp); fsw f19, 360(sp); fsw f20, 364(sp); fsw f21, 368(sp)
    fsw f22, 372(sp); fsw f23, 376(sp); fsw f24, 380(sp); fsw f25, 384(sp)
    fsw f26, 388(sp); fsw f27, 392(sp)
    frcsr t0; sw t0, 396(sp)
    csrr t0, mepc; sw t0, 404(sp)
    csrr t0, mcause; sw t0, 408(sp)
    csrr t0, mtval; sw t0, 412(sp)
    csrr t0, mscratch; sw t0, 416(sp)
    sw a0, 420(sp)
    sw sp, 424(sp)
    la t0, WS53_EXCEPTION_EXPECTED_SP
    sw sp, 0(t0)
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
        li t0, (0x3f000000+\n)
        fmv.w.x f\n, t0
    .endr
    li t0, 0x65; fscsr t0
    andi t1, a0, 0x30
    slli t1, t1, 9
    sw t1, 428(sp)
    srli t2, a0, 5
    andi t2, t2, 8
    sw t2, 432(sp)
    li t0, 0x13579bdf
    csrw mscratch, t0
    lw t0, 400(sp)
    li t3, ~0x7888
    and t0, t0, t3
    or t0, t0, t1
    or t0, t0, t2
    li t1, 0x1800
    or t0, t0, t1
    csrw mstatus, t0
    lw t0, 420(sp)
    andi t0, t0, 3
    beqz t0, .Lseed16
    li t1, 1
    beq t0, t1, .Lseed32
    j .Lseed_ecall

.macro seed_integer
    .irp n,1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
        li x\n, (0x53000000+\n)
    .endr
.endm
.Lseed16:
    seed_integer
.global WS53_EXC_FAULT16
WS53_EXC_FAULT16:
    .2byte 0
.global WS53_EXC_AFTER16
WS53_EXC_AFTER16:
    j .Lreturned
.Lseed32:
    seed_integer
.global WS53_EXC_FAULT32
WS53_EXC_FAULT32:
    .4byte 0xc0001073 # CSRW read-only CYCLE, guaranteed illegal in this model.
.global WS53_EXC_AFTER32
WS53_EXC_AFTER32:
    j .Lreturned
.Lseed_ecall:
    seed_integer
.global WS53_EXC_ECALL
WS53_EXC_ECALL:
    ecall
.global WS53_EXC_AFTER_ECALL
WS53_EXC_AFTER_ECALL:
    j .Lreturned

.Lreturned:
    sw zero, 0(sp)
    .irp n,1,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
        sw x\n, (4*\n)(sp)
    .endr
    sw sp, 8(sp)
    csrr t0, mstatus; sw t0, 128(sp)
    csrr t0, mscratch; sw t0, 132(sp)
    csrci mstatus, 8
    li t0, 0x6000; csrs mstatus, t0
    frcsr t0; sw t0, 148(sp)
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
        fsw f\n, (152+4*\n)(sp)
    .endr
    li a0, 0
    li t1, 1
.Lint_check:
    li t0, 2
    beq t1, t0, .Lint_next
    slli t0, t1, 2
    add t0, sp, t0
    lw t2, 0(t0)
    li t3, 0x53000000
    add t3, t3, t1
    beq t2, t3, .Lint_next
    addi a0, a0, 1
.Lint_next:
    addi t1, t1, 1
    li t0, 32
    bltu t1, t0, .Lint_check
    li t1, 0
.Lfloat_check:
    slli t0, t1, 2
    add t0, sp, t0
    lw t2, 152(t0)
    li t3, 0x3f000000
    add t3, t3, t1
    beq t2, t3, .Lfloat_next
    addi a0, a0, 1
.Lfloat_next:
    addi t1, t1, 1
    li t0, 32
    bltu t1, t0, .Lfloat_check
    lw t0, 148(sp)
    li t1, 0x65
    beq t0, t1, 1f
    addi a0, a0, 1
1:  lw t0, 128(sp)
    li t1, 0x6008
    and t0, t0, t1
    lw t1, 428(sp)
    lw t2, 432(sp)
    or t1, t1, t2
    beq t0, t1, 2f
    addi a0, a0, 1
2:  lw t0, 132(sp)
    li t1, 0x13579bdf
    beq t0, t1, 3f
    addi a0, a0, 1
3:  lw t0, 8(sp)
    lw t1, 424(sp)
    beq t0, t1, 4f
    addi a0, a0, 1
4:  # Restore the probe's own C ABI and pre-test CSR state.
    lw t0, 396(sp); fscsr t0
    flw f8, 348(sp); flw f9, 352(sp)
    flw f18, 356(sp); flw f19, 360(sp); flw f20, 364(sp); flw f21, 368(sp)
    flw f22, 372(sp); flw f23, 376(sp); flw f24, 380(sp); flw f25, 384(sp)
    flw f26, 388(sp); flw f27, 392(sp)
    lw t0, 404(sp); csrw mepc, t0
    lw t0, 408(sp); csrw mcause, t0
    lw t0, 412(sp); csrw mtval, t0
    lw t0, 416(sp); csrw mscratch, t0
    lw ra, 288(sp); lw gp, 292(sp); lw tp, 296(sp)
    lw s0, 300(sp); lw s1, 304(sp); lw s2, 308(sp); lw s3, 312(sp)
    lw s4, 316(sp); lw s5, 320(sp); lw s6, 324(sp); lw s7, 328(sp)
    lw s8, 332(sp); lw s9, 336(sp); lw s10, 340(sp); lw s11, 344(sp)
    lw t0, 400(sp); csrw mstatus, t0
    addi sp, sp, 448
    ret
.size ws53_exception_probe, .-ws53_exception_probe

.section .text.ws53_nested_fault, "ax"
.global ws53_nested_fault
ws53_nested_fault:
.global WS53_EXC_NESTED
WS53_EXC_NESTED:
    .2byte 0
.global WS53_EXC_AFTER_NESTED
WS53_EXC_AFTER_NESTED:
    ret
