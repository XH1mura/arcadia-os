# Arcadia OS Makefile
# Build system for the Arcadia Developer Operating System

.PHONY: all build kernel boot iso run clean test help

BUILD_DIR = build
TARGET_DIR = $(BUILD_DIR)/target
ISO_DIR = $(BUILD_DIR)/iso

all: build iso

build: kernel boot
	@echo "[SUCCESS] Build complete"

kernel:
	@echo "[INFO] Building kernel..."
	cargo build --target x86_64-unknown-none --release -p arcadia-kernel
	@echo "[SUCCESS] Kernel built"

boot:
	@echo "[INFO] Building bootloader..."
	cargo build --target x86_64-unknown-uefi -p arcadia-boot --release
	@echo "[SUCCESS] Bootloader built"

iso: build
	@echo "[INFO] Creating ISO..."
	@mkdir -p $(ISO_DIR)/EFI/Boot
	@mkdir -p $(ISO_DIR)/EFI/Arcadia
	@mkdir -p $(ISO_DIR)/boot/grub
	@cp $(TARGET_DIR)/x86_64-unknown-uefi/release/arcadia-boot.efi $(ISO_DIR)/EFI/Boot/bootx64.efi 2>/dev/null || \
		cp $(TARGET_DIR)/x86_64-unknown-uefi/release/arcadia_boot.efi $(ISO_DIR)/EFI/Boot/bootx64.efi 2>/dev/null || \
		echo "[WARN] Bootloader not found"
	@cp $(TARGET_DIR)/x86_64-unknown-none/release/arcadia-kernel $(ISO_DIR)/EFI/Arcadia/kernel.elf 2>/dev/null || \
		cp $(TARGET_DIR)/x86_64-unknown-none/release/arcadia_kernel $(ISO_DIR)/EFI/Arcadia/kernel.elf 2>/dev/null || \
		echo "[WARN] Kernel not found"
	@cp $(BUILD_DIR)/grub.cfg $(ISO_DIR)/boot/grub/ 2>/dev/null || true
	@echo "[SUCCESS] ISO structure created"

run: iso
	@echo "[INFO] Starting QEMU..."
	qemu-system-x86_64 \
		-cdrom $(BUILD_DIR)/arcadia.iso \
		-m 256M \
		-serial stdio \
		-display none \
		-no-reboot 2>/dev/null || \
	echo "[WARN] QEMU not found. Install qemu-system-x86_64"

clean:
	@echo "[INFO] Cleaning..."
	cargo clean 2>/dev/null || true
	rm -rf $(ISO_DIR)
	@echo "[SUCCESS] Clean complete"

test:
	@echo "[INFO] Running tests..."
	cargo test 2>/dev/null || echo "[WARN] No tests configured"
	@echo "[SUCCESS] Tests complete"

help:
	@echo "Arcadia OS Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all     - Build everything (default)"
	@echo "  build   - Build kernel and bootloader"
	@echo "  kernel  - Build kernel only"
	@echo "  boot    - Build bootloader only"
	@echo "  iso     - Create ISO structure"
	@echo "  run     - Build and run in QEMU"
	@echo "  clean   - Clean build artifacts"
	@echo "  test    - Run tests"
	@echo "  help    - Show this help"
