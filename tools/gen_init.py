#!/usr/bin/env python3
"""Generate a minimal ELF64 init binary for Arcadia OS Ring 3 testing."""
import struct
import sys

# ELF64 Header (64 bytes)
e_ident = b'\x7fELF'  # magic
e_ident += b'\x02'     # ELFCLASS64
e_ident += b'\x01'     # ELFDATA2LSB
e_ident += b'\x01'     # EV_CURRENT
e_ident += b'\x00'     # ELFOSABI_NONE
e_ident += b'\x00' * 8 # padding

e_type = 2             # ET_EXEC
e_machine = 0x3E       # EM_X86_64
e_version = 1
e_entry = 0x400000     # entry point virtual address
e_phoff = 64           # program header offset
e_shoff = 0            # section header offset
e_flags = 0
e_ehsize = 64
e_phentsize = 56       # sizeof(Elf64_Phdr)
e_phnum = 1
e_shentsize = 0
e_shnum = 0
e_shstrndx = 0

elf_header = e_ident
elf_header += struct.pack('<HH', e_type, e_machine)
elf_header += struct.pack('<I', e_version)
elf_header += struct.pack('<Q', e_entry)
elf_header += struct.pack('<Q', e_phoff)
elf_header += struct.pack('<Q', e_shoff)
elf_header += struct.pack('<I', e_flags)
elf_header += struct.pack('<HHH', e_ehsize, e_phentsize, e_phnum)
elf_header += struct.pack('<HHH', e_shentsize, e_shnum, e_shstrndx)

# Machine code
code = bytearray()

# Message data (placed after the code)
msg = b"Hello from Ring 3!\n"
msg_offset = 0 + 64 + 56 + len(msg)  # code_size unknown yet, put msg at known offset

# Actually, let's put msg inline after the code
# We need to know code size first. Let's assemble code with placeholder.

# _start:
#   ; sys_write(1, msg, msg_len)
#   mov rax, 1          ; SYS_WRITE
#   mov rdi, 1          ; fd = stdout
#   lea rsi, [msg]      ; buf
#   mov rdx, msg_len    ; len
#   int 0x80            ; syscall
#
#   ; sys_exit(0)
#   mov rax, 2          ; SYS_EXIT
#   xor rdi, rdi        ; code = 0
#   int 0x80            ; syscall

# Build code with relative offsets for the message
# We'll use: lea rsi, [rip + offset] pattern
# But simpler: put msg at fixed offset from code start

code_size = 30  # approximate, we'll fix after
msg_start = 64 + 56 + code_size  # ELF header + phdr + code
msg_size = len(msg)
total_file_size = msg_start + msg_size

# Build code
code = bytearray()
code += bytes([0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00])  # mov rax, 1
code += bytes([0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00])  # mov rdi, 1

# lea rsi, [rip + disp32] to point to msg
# RIP at end of this instruction = current_pos + 7 (instruction length)
# disp32 = msg_start - (current_pos + 7)
# current_pos for this instruction:
current_pos = len(code)
lea_instr_len = 7  # REX.W + LEA + ModRM + SIB + disp32
disp32 = msg_start - (current_pos + lea_instr_len)
code += bytes([0x48, 0x8D, 0x35])  # lea rsi, [rip + ...]
code += struct.pack('<i', disp32)

code += bytes([0x48, 0xC7, 0xC2])  # mov rdx, msg_len
code += struct.pack('<I', msg_size)

code += bytes([0xCD, 0x80])  # int 0x80

code += bytes([0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00])  # mov rax, 2
code += bytes([0x48, 0x31, 0xFF])  # xor rdi, rdi
code += bytes([0xCD, 0x80])  # int 0x80

# Pad code to fill up to code_size
actual_code_size = len(code)
code += b'\x90' * (code_size - actual_code_size)  # nop padding

# Now recalculate with actual code size if needed
# Actually the msg offset should be 64 + 56 + actual_code_size
msg_start_actual = 64 + 56 + actual_code_size
# Rebuild code with correct displacement
code = bytearray()
code += bytes([0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00])  # mov rax, 1
code += bytes([0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00])  # mov rdi, 1

current_pos = len(code)
lea_instr_len = 7
disp32 = msg_start_actual - (current_pos + lea_instr_len)
code += bytes([0x48, 0x8D, 0x35])  # lea rsi, [rip + ...]
code += struct.pack('<i', disp32)

code += bytes([0x48, 0xC7, 0xC2])  # mov rdx, msg_len
code += struct.pack('<I', msg_size)

code += bytes([0xCD, 0x80])  # int 0x80

code += bytes([0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00])  # mov rax, 2
code += bytes([0x48, 0x31, 0xFF])  # xor rdi, rdi
code += bytes([0xCD, 0x80])  # int 0x80

total_code_size = len(code)
total_file_size = 64 + 56 + total_code_size + msg_size

# Program header for .text (LOAD, R+X)
p_type = 1             # PT_LOAD
p_flags = 5            # PF_R | PF_X
p_offset = 0           # load from start of file
p_vaddr = 0x400000
p_paddr = 0x400000
p_filesz = total_file_size
p_memsz = total_file_size
p_align = 0x1000

phdr = struct.pack('<IIQQQQQQ', p_type, p_flags, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_align)

# Assemble
output = elf_header + phdr + code + msg

assert len(elf_header) == 64
assert len(phdr) == 56
assert len(output) == total_file_size

with open(sys.argv[1], 'wb') as f:
    f.write(output)

print(f"Generated init ELF: {total_file_size} bytes")
print(f"  Code: {total_code_size} bytes at offset {64+56}")
print(f"  Data: {msg_size} bytes at offset {64+56+total_code_size}")
print(f"  Entry: 0x{e_entry:X}")
