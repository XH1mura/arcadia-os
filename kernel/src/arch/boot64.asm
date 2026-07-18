; Arcadia OS - PVH boot stub (x86_64 ELF64)
; Booted via QEMU -kernel with XEN_ELFNOTE_PHYS32_ENTRY note.
; CPU enters in 32-bit protected mode. We set up long mode and jump to 64-bit.

; -- PVH ELF Note (placed in .note section so QEMU finds it in PT_NOTE) ------
section .note.Xen note
    dd 4                        ; namesz ("Xen\0")
    dd 8                        ; descsz (64-bit entry point)
    dd 18                       ; type = XEN_ELFNOTE_PHYS32_ENTRY
    db 'X', 'e', 'n', 0
    dq _pvh_start               ; entry point (linker resolves)

; -- 32-bit code (PVH entry) ------------------------------------------------
section .text.boot
bits 32
global _pvh_start
extern _bss_start
extern _bss_end
extern arcadia_kernel_main

; -- GDT (must be accessible from both 32 and 64-bit) ------------------------
align 8
gdt64:
    dq 0                        ; null
    dw 0xFFFF, 0                ; code base=0, limit=4G
    db 0, 10011010b, 10101111b, 0
    dw 0xFFFF, 0                ; data base=0, limit=4G
    db 0, 10010010b, 11001111b, 0
gdt64_end:
gdt64_ptr:
    dw gdt64_end - gdt64 - 1
    dq gdt64

; -- PVH Entry Point (32-bit protected mode) ---------------------------------
_pvh_start:
    cli

    ; Save HVM start info data to safe location before page table setup overwrites it.
    ; Copy 256 bytes (enough for header + ~9 memmap entries) to 0x500.
    mov esi, ebx            ; ESI = source (HVM start info pointer)
    mov edi, 0x500          ; EDI = destination (safe area before page tables)
    mov ecx, 64             ; 64 dwords = 256 bytes
    rep movsd               ; copy HVM data to safe location

    ; Load GDT
    lgdt [gdt64_ptr]

    ; Reload segment registers with 64-bit data segment
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Enable PAE (CR4 bit 5)
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax

    ; Build identity-mapped page tables at 0x1000
    ; PML4 at 0x1000 -> PDP at 0x2000
    ; PDP at 0x2000 -> PD at 0x3000
    ; PD at 0x3000: 2 MiB pages for 0-2 MiB and 2-4 MiB (VGA at 0xB8000)
    mov edi, 0x1000
    mov ecx, 4096 * 3 / 4
    xor eax, eax
    rep stosd

    mov dword [0x1000], 0x2003       ; PML4[0] -> PDP
    mov dword [0x2000], 0x3003       ; PDP[0] -> PD
    mov dword [0x3000], 0x00000083   ; PD[0] = 0 MiB (2 MiB page)
    mov dword [0x3008], 0x00200083   ; PD[1] = 2 MiB (2 MiB page, covers VGA)
    mov dword [0x3010], 0x00400083   ; PD[2] = 4 MiB (2 MiB page, covers heap)

    ; Load PML4 into CR3
    mov eax, 0x1000
    mov cr3, eax

    ; Enable long mode (EFER.LME, MSR 0xC0000080 bit 8)
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    ; Enable paging (CR0.PG) and protected mode (CR0.PE)
    mov eax, cr0
    or eax, (1 << 31) | (1 << 0)
    mov cr0, eax

    ; Far jump to 64-bit code
    jmp 0x08:long_mode

; -- 64-bit code -------------------------------------------------------------
bits 64
long_mode:
    mov rsp, 0x80000           ; stack below 1 MiB

    ; Restore HVM start info pointer to saved copy at 0x500
    mov rbx, 0x500

    ; Clear BSS (handle both 8-byte chunks and remaining bytes)
    mov rdi, _bss_start
    mov rcx, _bss_end
    sub rcx, rdi
    ; Clear 8-byte chunks
    mov r8, rcx
    shr rcx, 3
    xor rax, rax
    rep stosq
    ; Clear remaining bytes (0-7)
    and r8, 7
    mov rcx, r8
    rep stosb

    ; Call arcadia_kernel_main(hvm_info, 0)
    ; RBX = HVM start info pointer from PVH bootloader (preserved through mode switch)
    mov rdi, rbx
    xor rsi, rsi
    call arcadia_kernel_main

    cli
.halt:
    hlt
    jmp .halt
