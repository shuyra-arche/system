OUTPUT_ARCH(riscv)
ENTRY(_start)
PHDRS { code PT_LOAD FLAGS(5); data PT_LOAD FLAGS(6); stack PT_LOAD FLAGS(6); }
SECTIONS {
    . = 0x40000000;
    .text : {
        __code_start = .;
        KEEP(*(.text.entry))
        *(.text .text.*) *(.rodata .rodata.* .srodata .srodata.*)
        __code_end = .;
    } :code
    . = ALIGN(4096);
    .data : {
        PROVIDE(__global_pointer$ = . + 0x800);
        *(.data .data.* .sdata .sdata.*)
    } :data
    .bss : { *(.bss .bss.* .sbss .sbss.*) } :data
    /* The initial Kernel stack is only for the startup argument handoff.
       Keep a page absent from every PT_LOAD below System's lifetime stack. */
    . = ALIGN(4096) + 4096;
    .stack (NOLOAD) : {
        __system_stack_bottom = .;
        . += 1024 * 1024;
        __system_stack_top = .;
    } :stack
    /DISCARD/ : { *(.eh_frame*) }
}
