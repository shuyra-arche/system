OUTPUT_ARCH(riscv)
ENTRY(_start)
PHDRS { code PT_LOAD FLAGS(5); data PT_LOAD FLAGS(6); }
SECTIONS {
    . = 0x40000000;
    .text : { __code_start = .; KEEP(*(.text.entry)) *(.text .text.*) *(.rodata .rodata.* .srodata .srodata.*) __code_end = .; } :code
    . = ALIGN(4096);
    .data : { *(.data .data.* .sdata .sdata.*) } :data
    .bss : { *(.bss .bss.* .sbss .sbss.*) } :data
    /DISCARD/ : { *(.eh_frame*) }
}
