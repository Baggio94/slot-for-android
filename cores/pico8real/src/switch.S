// Stack switching between the host and PICO-8, and the few imports that
// must be assembly: the yields (bound to SDL_RenderPresent and
// SDL_UpdateWindowSurface), exit, and setjmp/longjmp that keep raw (unmangled)
// sp and lr so a jmp_buf stays valid in a later process.
//
// struct p8ctx: x19..x28 at 0, fp 80, lr 88, sp 96, d8..d15 104, x18 168.
// pico8_64 (gcc, Linux) uses x18 as a scratch register; Android reserves it
// for the shadow call stack, so the host's x18 is saved on every way into
// PICO-8 and restored on every way out.

#define P8_SHIM_CTX 0x2001010010   /* &SHIM->ctx: P8_SHIM + offsetof(ctx) */

    .text
    .p2align 4

    .globl p8_switch
    .type p8_switch, %function
// uint64_t p8_switch(struct p8ctx *save, struct p8ctx *load, uint64_t value)
p8_switch:
    stp x19, x20, [x0, #0]
    stp x21, x22, [x0, #16]
    stp x23, x24, [x0, #32]
    stp x25, x26, [x0, #48]
    stp x27, x28, [x0, #64]
    stp x29, x30, [x0, #80]
    mov x9, sp
    str x9, [x0, #96]
    stp d8, d9, [x0, #104]
    stp d10, d11, [x0, #120]
    stp d12, d13, [x0, #136]
    stp d14, d15, [x0, #152]
    str x18, [x0, #168]
    ldp x19, x20, [x1, #0]
    ldp x21, x22, [x1, #16]
    ldp x23, x24, [x1, #32]
    ldp x25, x26, [x1, #48]
    ldp x27, x28, [x1, #64]
    ldp x29, x30, [x1, #80]
    ldr x9, [x1, #96]
    mov sp, x9
    ldp d8, d9, [x1, #104]
    ldp d10, d11, [x1, #120]
    ldp d12, d13, [x1, #136]
    ldp d14, d15, [x1, #152]
    ldr x18, [x1, #168]
    mov x0, x2
    ret
    .size p8_switch, .-p8_switch

.macro shim_ctx reg
    movz \reg, #0x0010
    movk \reg, #0x0101, lsl #16
    movk \reg, #0x0020, lsl #32
.endm

// Called by PICO-8 in place of SDL_RenderPresent: hand the frame to the host.
    .globl p8_yield_present
    .type p8_yield_present, %function
p8_yield_present:
    stp x29, x30, [sp, #-16]!   // the host's pre-present work runs and returns
    mov x29, sp                 // before the switch: no host frame stays behind
    bl p8_pre_present
    ldp x29, x30, [sp], #16
    shim_ctx x0
    adrp x1, p8_host_ctx
    add x1, x1, :lo12:p8_host_ctx
    mov x2, #1          /* P8_FRAME */
    b p8_switch
    .size p8_yield_present, .-p8_yield_present

// SDL_UpdateWindowSurface (the software renderer's present): returns 0.
    .globl p8_yield_window
    .type p8_yield_window, %function
p8_yield_window:
    stp x29, x30, [sp, #-16]!   // the host's pre-present work runs and returns
    mov x29, sp                 // before the switch: no host frame stays behind
    bl p8_pre_window
    ldp x29, x30, [sp], #16
    shim_ctx x0
    adrp x1, p8_host_ctx
    add x1, x1, :lo12:p8_host_ctx
    mov x2, #1
    b p8_switch
    .size p8_yield_window, .-p8_yield_window

// SDL_Delay: time passes; a PICO-8 that waits a while without presenting
// (a download, a cart loading) gives the host its turn back as P8_IDLE, so
// the host never hangs on it. Like the presents, nothing of the host stays
// on PICO-8's stack: it resumes straight into PICO-8.
    .globl p8_yield_delay
    .type p8_yield_delay, %function
p8_yield_delay:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    bl p8_pre_delay             // 1: give the host its turn
    ldp x29, x30, [sp], #16
    cbz x0, 1f
    shim_ctx x0
    adrp x1, p8_host_ctx
    add x1, x1, :lo12:p8_host_ctx
    mov x2, #3          /* P8_IDLE */
    b p8_switch
1:  ret
    .size p8_yield_delay, .-p8_yield_delay

// exit(): PICO-8 is done; the host sees P8_EXIT on this and every later frame.
    .globl p8_exit
    .type p8_exit, %function
p8_exit:
    adrp x9, p8_exit_code
    str w0, [x9, :lo12:p8_exit_code]
1:  shim_ctx x0
    adrp x1, p8_host_ctx
    add x1, x1, :lo12:p8_host_ctx
    mov x2, #2          /* P8_EXIT */
    bl p8_switch
    b 1b
    .size p8_exit, .-p8_exit

// glibc's jmp_buf is 312 bytes; we use the first 176.
    .globl p8_setjmp
    .type p8_setjmp, %function
p8_setjmp:
    stp x19, x20, [x0, #0]
    stp x21, x22, [x0, #16]
    stp x23, x24, [x0, #32]
    stp x25, x26, [x0, #48]
    stp x27, x28, [x0, #64]
    stp x29, x30, [x0, #80]
    mov x9, sp
    str x9, [x0, #96]
    stp d8, d9, [x0, #104]
    stp d10, d11, [x0, #120]
    stp d12, d13, [x0, #136]
    stp d14, d15, [x0, #152]
    mov w0, #0
    ret
    .size p8_setjmp, .-p8_setjmp

    .globl p8_longjmp
    .type p8_longjmp, %function
p8_longjmp:
    ldp x19, x20, [x0, #0]
    ldp x21, x22, [x0, #16]
    ldp x23, x24, [x0, #32]
    ldp x25, x26, [x0, #48]
    ldp x27, x28, [x0, #64]
    ldp x29, x30, [x0, #80]
    ldr x9, [x0, #96]
    mov sp, x9
    ldp d8, d9, [x0, #104]
    ldp d10, d11, [x0, #120]
    ldp d12, d13, [x0, #136]
    ldp d14, d15, [x0, #152]
    cmp w1, #0
    csinc w0, w1, wzr, ne
    ret
    .size p8_longjmp, .-p8_longjmp

// uint64_t p8_call_on(uint64_t region, void *fn, uint64_t a, uint64_t b, uint64_t c):
// call PICO-8 code on the 16 MB stack region at `region`, keeping the
// caller's x18 at the region's base for the import thunks.
    .globl p8_call_on
    .type p8_call_on, %function
p8_call_on:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    stp x18, x19, [sp, #16]
    str x18, [x0]               /* the region's x18 slot */
    mov x19, sp
    mov x9, #0x1000000
    add x9, x0, x9              /* top of the region */
    mov sp, x9
    mov x9, x1
    mov x0, x2
    mov x1, x3
    mov x2, x4
    blr x9
    mov sp, x19
    ldp x18, x19, [sp, #16]
    ldp x29, x30, [sp], #32
    ret
    .size p8_call_on, .-p8_call_on

    .globl p8_get_x18
    .type p8_get_x18, %function
p8_get_x18:
    mov x0, x18
    ret
    .size p8_get_x18, .-p8_get_x18

// An import thunk, copied once per import into the fixed page: restore the
// host's x18 from the base of the stack region PICO-8 is running on, then
// jump to the import (the literal, written per copy). 32 bytes.
    .p2align 3
    .globl p8_thunk_template
    .globl p8_thunk_template_end
p8_thunk_template:
    mov x16, sp
    and x16, x16, #0xffffffffff000000
    ldr x18, [x16]
    ldr x16, 1f
    br x16
    nop
1:  .quad 0
p8_thunk_template_end:

    .bss
    .p2align 4
    .globl p8_host_ctx
p8_host_ctx:
    .zero 184
    .globl p8_exit_code
p8_exit_code:
    .zero 8

    .section .note.GNU-stack,"",%progbits
