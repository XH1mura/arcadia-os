pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const ELFCLASS64: u8 = 2;
pub const ELFDATA2LSB: u8 = 1;
pub const ET_EXEC: u16 = 2;
pub const EM_X86_64: u16 = 0x3E;
pub const PT_LOAD: u32 = 1;
pub const PT_NOTE: u32 = 4;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

impl Elf64Header {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 64 {
            return None;
        }
        if data[..4] != ELF_MAGIC {
            return None;
        }
        if data[4] != ELFCLASS64 || data[5] != ELFDATA2LSB {
            return None;
        }
        let header = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const Elf64Header) };
        if header.e_type != ET_EXEC || header.e_machine != EM_X86_64 {
            return None;
        }
        Some(header)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

impl Elf64ProgramHeader {
    pub fn is_loadable(&self) -> bool {
        self.p_type == PT_LOAD
    }
    pub fn is_executable(&self) -> bool {
        self.p_flags & 0x1 != 0
    }
    pub fn is_writable(&self) -> bool {
        self.p_flags & 0x2 != 0
    }
}

pub struct ElfLoadInfo {
    pub entry: u64,
    pub segments: [ElfSegment; 4],
    pub segment_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ElfSegment {
    pub vaddr: u64,
    pub memsz: u64,
    pub filesz: u64,
    pub data_offset: u64,
    pub writable: bool,
    pub executable: bool,
}

pub fn parse_elf(data: &[u8]) -> Result<ElfLoadInfo, &'static str> {
    let header = Elf64Header::parse(data).ok_or("Invalid ELF header")?;

    let phoff = header.e_phoff as usize;
    let phentsize = header.e_phentsize as usize;
    let phnum = header.e_phnum as usize;

    if phnum > 4 {
        return Err("Too many program headers");
    }

    let mut info = ElfLoadInfo {
        entry: header.e_entry,
        segments: [ElfSegment {
            vaddr: 0,
            memsz: 0,
            filesz: 0,
            data_offset: 0,
            writable: false,
            executable: false,
        }; 4],
        segment_count: 0,
    };

    for i in 0..phnum {
        let offset = phoff + i * phentsize;
        if offset + phentsize > data.len() {
            return Err("Program header out of bounds");
        }
        let ph = unsafe {
            core::ptr::read_unaligned(data[offset..].as_ptr() as *const Elf64ProgramHeader)
        };

        if ph.is_loadable() {
            if info.segment_count >= 4 {
                return Err("Too many loadable segments");
            }
            info.segments[info.segment_count] = ElfSegment {
                vaddr: ph.p_vaddr,
                memsz: ph.p_memsz,
                filesz: ph.p_filesz,
                data_offset: ph.p_offset,
                writable: ph.is_writable(),
                executable: ph.is_executable(),
            };
            info.segment_count += 1;
        }
    }

    if info.segment_count == 0 {
        return Err("No loadable segments");
    }

    Ok(info)
}
