OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_pvh_start)

SECTIONS {
    . = 0x00100000;

    .note.Xen : ALIGN(4) {
        *(.note.Xen)
    }

    .text.boot : ALIGN(4K) {
        *(.text.boot)
    }

    .text : ALIGN(4K) {
        *(.text)
    }

    .rodata : ALIGN(4K) {
        *(.rodata)
    }

    .data : ALIGN(4K) {
        *(.data)
    }

    .bss : ALIGN(4K) {
        _bss_start = .;
        *(.bss)
        *(.bss.*)
        *(COMMON)
        _bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.debug*)
    }
}
