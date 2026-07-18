<div align="center">

<img src="assets/logo.gif" width="220" alt="Arcadia OS"/>

# Arcadia OS

### Developer-First Operating System

*A modern 64-bit operating system built from scratch in Rust and x86_64 Assembly.*

![Version](https://img.shields.io/badge/version-v0.2--alpha-4c8bf5?style=for-the-badge)
![Architecture](https://img.shields.io/badge/architecture-x86__64-orange?style=for-the-badge)
![Kernel](https://img.shields.io/badge/kernel-Monolithic-red?style=for-the-badge)
![Language](https://img.shields.io/badge/language-Rust%20%2B%20Assembly-black?style=for-the-badge)
![License](https://img.shields.io/badge/license-MIT-green?style=for-the-badge)
![Status](https://img.shields.io/badge/status-Active%20Development-success?style=for-the-badge)

---

**Minimal • Fast • Reliable • Developer Focused**

</div>

---

# About

Arcadia OS is a modern operating system developed completely from scratch using **Rust** and **x86_64 Assembly**.

The project is built around one philosophy:

> **The operating system should empower developers instead of getting in their way.**

Arcadia is designed to be lightweight, maintainable and predictable. Every subsystem exists because it solves a real problem—not because every operating system is expected to have it.

Unlike traditional desktop operating systems, Arcadia begins as a **terminal-first environment**, providing a clean platform for software development, systems programming and operating system research.

This project is **not a Linux distribution**.

This project is **not based on another kernel**.

Everything—from the boot sequence to the filesystem—is built specifically for Arcadia.

---

# Philosophy

Arcadia follows several engineering principles.

- Minimal by Design
- Reliability over Features
- Explicit Architecture
- Zero Placeholder Code
- Production Quality
- Long-Term Maintainability
- Memory Safety whenever possible
- Clean Internal APIs
- Modular Subsystems
- Predictable Behaviour

Every subsystem must justify its existence.

---

# Current Features

## Boot

- PVH Boot Protocol
- Custom Assembly Boot Stub
- 64-bit Long Mode Initialization
- ELF64 Kernel Boot
- Identity-Mapped Memory
- Boot Logging
- Panic Recovery

---

## CPU & Architecture

- x86_64 Architecture
- Global Descriptor Table (GDT)
- Interrupt Descriptor Table (IDT)
- Task State Segment (TSS)
- Interrupt Stack Table (IST)
- Ring 0 Execution
- Ring 3 Foundation
- INT 0x80 System Calls

---

## Memory Management

- Physical Memory Manager (PMM)
- Bitmap Frame Allocator
- Virtual Memory Manager (VMM)
- Four-Level Paging
- Dynamic Page Mapping
- Page Translation
- Region Mapping
- TLB Management
- Runtime Validation
- Memory Statistics

---

## Interrupts & Timing

- Programmable Interrupt Controller (8259 PIC)
- Programmable Interval Timer (8254 PIT)
- Timer Interrupts
- Keyboard Interrupts
- Serial Interrupts
- Page Fault Handler
- Double Fault Recovery

---

## Hardware Detection

- CPUID
- Memory Discovery
- PCI Enumeration
- Multi-Function PCI Support
- Device Framework
- Hardware Abstraction Layer (HAL)

---

## Storage

- ATA PIO Driver
- Block Device Layer
- Master Boot Record (MBR)
- Partition Detection
- FAT32 Filesystem
- Cluster Allocation
- Directory Management
- File Reading
- File Writing
- File Deletion

---

## Virtual File System

- Mount Points
- Path Resolution
- File Descriptors
- Current Working Directory
- Recursive Directory Traversal
- File Statistics
- Unified Filesystem Interface

---

## Process Management

- ELF64 Loader
- Process Control Block
- User Address Space
- Context Saving
- Context Restore
- Ring 3 Transition
- Process Cleanup
- Kernel/User Separation

---

## Drivers

- ATA
- PCI
- VGA Text Console
- PS/2 Keyboard
- Serial Port

---

## ArcShell

Built-in kernel shell with filesystem support.

Current commands include:

```text
help
version
clear
echo
hostname
whoami

cpuinfo
mem
meminfo
sysinfo
uptime

disk
disk r
disk w
partitions
pci
vmm

mount
pwd
cd
ls
cat
touch
mkdir
write
rm

reboot
halt
exit
```

Every command is expected to be fully functional before release.

---

# Architecture

```text
+--------------------------------------+
|          User Applications           |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|           ELF64 Loader               |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|          System Call Layer           |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|       Process Manager / Scheduler    |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|      Virtual File System (VFS)       |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|         Driver Framework             |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|     Hardware Abstraction Layer       |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|      Arcadia Kernel (Rust)           |
+--------------------------------------+
                 │
                 ▼
+--------------------------------------+
|          x86_64 Hardware             |
+--------------------------------------+
```

---

# Project Structure

```text
Arcadia
│
├── boot/
│
├── kernel/
│   ├── arch/
│   ├── memory/
│   ├── drivers/
│   ├── fs/
│   ├── block/
│   ├── process/
│   ├── terminal/
│   ├── interrupts/
│   └── pci/
│
├── assets/
│
├── scripts/
│
└── start.ps1
```

---

# Technology Stack

| Component | Technology |
|------------|------------|
| Language | Rust |
| Low Level | x86_64 Assembly |
| Architecture | x86_64 |
| Boot Protocol | PVH |
| Executable Format | ELF64 |
| Build System | Cargo |
| Assembler | NASM |
| Linker | rust-lld |
| Emulator | QEMU |

---

# Development Roadmap

| Component | Status |
|------------|---------|
| Bootloader | ✅ Complete |
| Kernel Foundation | ✅ Complete |
| Interrupt System | ✅ Complete |
| PMM | ✅ Complete |
| VMM | ✅ Complete |
| PCI Enumeration | ✅ Complete |
| Driver Framework | ✅ Complete |
| ATA Driver | ✅ Complete |
| Block Device Layer | ✅ Complete |
| MBR Parser | ✅ Complete |
| FAT32 | ✅ Complete |
| Virtual File System | ✅ Complete |
| ArcShell | ✅ Complete |
| ELF Loader | 🚧 In Progress |
| Process Manager | 🚧 In Progress |
| Scheduler | 🚧 In Progress |
| Userspace | 🚧 In Progress |
| Networking | ⏳ Planned |
| USB Stack | ⏳ Planned |
| AHCI | ⏳ Planned |
| NVMe | ⏳ Planned |
| Audio | ⏳ Planned |
| Bluetooth | ⏳ Planned |
| ArcWM | ⏳ Planned |
| GUI | ⏳ Planned |
| Flux Package Manager | ⏳ Planned |
| SDK | ⏳ Planned |
| Arcadia 1.0 | ⏳ Planned |

---

# Future Goals

### Storage

- AHCI
- NVMe
- USB Mass Storage
- Optical Drives
- exFAT
- EXT4 (Read Only)
- ArcFS

### Networking

- Ethernet
- IPv4
- IPv6
- DHCP
- DNS
- TCP
- UDP
- ICMP

### Security

- User Permissions
- Process Isolation
- Memory Protection
- Secure Syscalls

### Developer Tools

- ArcShell
- ArcEdit
- Flux Package Manager
- SDK
- Build Tools
- Debug Tools

### Desktop

- ArcWM
- GPU Acceleration
- Wayland-Inspired Compositor
- PNG Boot Animation
- Modern Terminal UI

---

# Engineering Standards

Arcadia follows strict engineering rules.

The project does **not** allow:

- Placeholder implementations
- Stub functions
- Fake drivers
- Fake hardware detection
- TODO code
- Disabled features presented as complete
- Untested functionality

Every new subsystem must:

- Build successfully
- Boot successfully
- Pass regression tests
- Preserve kernel stability
- Follow project architecture
- Maintain memory safety
- Include proper error handling

---

# Building

Clone the repository:

```bash
git clone https://github.com/XH1mura/arcadia-os.git
cd arcadia-os
```

Build the operating system:

```bash
powershell -ExecutionPolicy Bypass -File start.ps1 build
```

Run with QEMU:

```bash
powershell -ExecutionPolicy Bypass -File start.ps1 run
```

Clean build artifacts:

```bash
powershell -ExecutionPolicy Bypass -File start.ps1 clean
```

---

# Requirements

- Rust Nightly
- Cargo
- NASM
- LLVM (rust-lld)
- QEMU
- PowerShell 7+

---

# Contributing

Contributions are welcome.

Before opening a Pull Request, ensure that:

- The project builds successfully.
- The kernel boots without regressions.
- Existing functionality remains intact.
- New functionality includes proper validation.
- Code follows the existing architecture.

Quality is preferred over quantity.

---

# License

This project is licensed under the MIT License.

See the [LICENSE](LICENSE) file for details.

---

<div align="center">

## Arcadia OS

### Build Software. Not Complexity.

**Created by XH1mura**

Made with ❤️ in Kazakhstan 🇰🇿

© 2026 Arcadia OS Project

</div>
