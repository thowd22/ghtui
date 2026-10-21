//! Tiny native executables that stand in for ghtui in installer tests.
//!
//! scripts/install.sh reads the ELF/Mach-O header of a downloaded binary
//! before it runs it, so a shell-script stand-in would be refused. These
//! helpers emit minimal static ELF64 programs (no libc, raw syscalls) for
//! x86_64 and aarch64 Linux. Each prints `message` whatever its arguments
//! (so `--version` reports `ghtui X.Y.Z`, and the bytes contain "ghtui" for
//! the installer's ownership check), optionally creates a marker file first
//! (a tripwire that proves whether it ever ran) or sends SIGTERM to its
//! parent (an installer interrupted mid-verification), and exits 0.
#![allow(dead_code)]

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl Arch {
    pub fn host() -> Self {
        if cfg!(target_arch = "aarch64") {
            Self::Aarch64
        } else if cfg!(target_arch = "x86_64") {
            Self::X86_64
        } else {
            panic!("installer stand-ins exist for x86_64 and aarch64 hosts only")
        }
    }
    /// The other supported CPU: its binaries cannot run on this host.
    pub fn foreign() -> Self {
        match Self::host() {
            Self::X86_64 => Self::Aarch64,
            Self::Aarch64 => Self::X86_64,
        }
    }
    pub fn linux_target(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-linux-musl",
            Self::Aarch64 => "aarch64-unknown-linux-musl",
        }
    }
    pub fn darwin_target(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-apple-darwin",
            Self::Aarch64 => "aarch64-apple-darwin",
        }
    }
    /// What `uname -m` prints on Linux for this CPU.
    pub fn uname(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }
}

#[derive(Default)]
pub struct Program<'a> {
    pub message: &'a str,
    pub touch: Option<&'a Path>,
    pub kill_parent: bool,
}

const BASE: u64 = 0x40_0000;
const CODE_OFFSET: u64 = 64 + 56;

/// A runnable static ELF64 little-endian executable for `arch`.
pub fn elf(arch: Arch, program: &Program) -> Vec<u8> {
    // Instruction sizes never depend on the data addresses, so one pass with
    // placeholder addresses fixes where the data starts.
    let code_len = code(arch, program, 0, 0).len() as u64;
    let message_addr = BASE + CODE_OFFSET + code_len;
    let path_addr = message_addr + program.message.len() as u64;
    let code = code(arch, program, message_addr as u32, path_addr as u32);
    let mut data = program.message.as_bytes().to_vec();
    if let Some(path) = program.touch {
        use std::os::unix::ffi::OsStrExt;
        data.extend_from_slice(path.as_os_str().as_bytes());
        data.push(0);
    }
    let total = CODE_OFFSET + code.len() as u64 + data.len() as u64;

    let mut out = Vec::new();
    out.extend_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    let machine: u16 = match arch {
        Arch::X86_64 => 62,
        Arch::Aarch64 => 183,
    };
    out.extend_from_slice(&machine.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(BASE + CODE_OFFSET).to_le_bytes()); // entry
    out.extend_from_slice(&64u64.to_le_bytes()); // program headers
    out.extend_from_slice(&0u64.to_le_bytes()); // no section headers
    out.extend_from_slice(&0u32.to_le_bytes());
    for half in [64u16, 56, 1, 0, 0, 0] {
        out.extend_from_slice(&half.to_le_bytes());
    }
    // One PT_LOAD segment, read + execute, mapping the whole file.
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&5u32.to_le_bytes());
    for word in [0, BASE, BASE, total, total, 0x1_0000] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    assert_eq!(out.len() as u64, CODE_OFFSET);
    out.extend_from_slice(&code);
    out.extend_from_slice(&data);
    out
}

fn code(arch: Arch, program: &Program, message: u32, path: u32) -> Vec<u8> {
    let len = program.message.len() as u32;
    match arch {
        Arch::X86_64 => {
            let mut c = Vec::new();
            let mov = |c: &mut Vec<u8>, op: u8, value: u32| {
                c.push(op);
                c.extend_from_slice(&value.to_le_bytes());
            };
            const EAX: u8 = 0xb8;
            const EDX: u8 = 0xba;
            const ESI: u8 = 0xbe;
            const EDI: u8 = 0xbf;
            const SYSCALL: [u8; 2] = [0x0f, 0x05];
            if program.touch.is_some() {
                mov(&mut c, EAX, 85); // creat(path, 0644)
                mov(&mut c, EDI, path);
                mov(&mut c, ESI, 0o644);
                c.extend_from_slice(&SYSCALL);
            }
            if program.kill_parent {
                mov(&mut c, EAX, 110); // getppid()
                c.extend_from_slice(&SYSCALL);
                c.extend_from_slice(&[0x89, 0xc7]); // mov edi, eax
                mov(&mut c, ESI, 15);
                mov(&mut c, EAX, 62); // kill(ppid, SIGTERM)
                c.extend_from_slice(&SYSCALL);
            }
            mov(&mut c, EAX, 1); // write(1, message, len)
            mov(&mut c, EDI, 1);
            mov(&mut c, ESI, message);
            mov(&mut c, EDX, len);
            c.extend_from_slice(&SYSCALL);
            mov(&mut c, EAX, 60); // exit(0)
            c.extend_from_slice(&[0x31, 0xff]);
            c.extend_from_slice(&SYSCALL);
            c
        }
        Arch::Aarch64 => {
            let movz =
                |rd: u32, imm: u32, hw: u32| 0xd280_0000 | (hw << 21) | ((imm & 0xffff) << 5) | rd;
            let movk =
                |rd: u32, imm: u32, hw: u32| 0xf280_0000 | (hw << 21) | ((imm & 0xffff) << 5) | rd;
            let movn = |rd: u32, imm: u32| 0x9280_0000 | ((imm & 0xffff) << 5) | rd;
            const SVC: u32 = 0xd400_0001;
            let mut words = Vec::new();
            let load = |words: &mut Vec<u32>, rd: u32, value: u32| {
                words.push(movz(rd, value, 0));
                words.push(movk(rd, value >> 16, 1));
            };
            if program.touch.is_some() {
                words.push(movn(0, 99)); // AT_FDCWD = -100
                load(&mut words, 1, path);
                words.push(movz(2, 0o101, 0)); // O_WRONLY | O_CREAT
                words.push(movz(3, 0o644, 0));
                words.push(movz(8, 56, 0)); // openat
                words.push(SVC);
            }
            if program.kill_parent {
                words.push(movz(8, 173, 0)); // getppid -> x0
                words.push(SVC);
                words.push(movz(1, 15, 0));
                words.push(movz(8, 129, 0)); // kill(x0, SIGTERM)
                words.push(SVC);
            }
            words.push(movz(0, 1, 0));
            load(&mut words, 1, message);
            words.push(movz(2, len, 0));
            words.push(movz(8, 64, 0)); // write
            words.push(SVC);
            words.push(movz(0, 0, 0));
            words.push(movz(8, 93, 0)); // exit
            words.push(SVC);
            words.iter().flat_map(|word| word.to_le_bytes()).collect()
        }
    }
}

/// A thin 64-bit Mach-O header for `arch` followed by `message`. It is not
/// runnable anywhere; it lets a Linux host check the installer's macOS
/// target mapping up to the point where the binary would run.
pub fn macho(arch: Arch, message: &str) -> Vec<u8> {
    let cpu: u32 = match arch {
        Arch::X86_64 => 0x0100_0007,
        Arch::Aarch64 => 0x0100_000c,
    };
    let mut out = Vec::new();
    for word in [0xfeed_facf_u32, cpu, 0, 2, 0, 0, 0, 0] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(message.as_bytes());
    out
}
