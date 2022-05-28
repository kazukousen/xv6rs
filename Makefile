CARGO_TARGET = riscv64imac-unknown-none-elf
TARGET = target/$(CARGO_TARGET)/debug
ifdef CARGO_RELEASE
	RELEASE = --release
	TARGET = target/$(CARGO_TARGET)/release
endif

TARGET_BIN = $(TARGET)/xv6rs-kernel

CARGO = cargo
CARGO_BUILD = $(CARGO) build $(RELEASE) --target $(CARGO_TARGET)

$(TARGET_BIN):
	RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_BUILD) -p xv6rs-kernel

.PHONY: qemu
qemu: $(TARGET_BIN)
	qemu-system-riscv64 \
    -M virt \
    -bios none \
    -nographic \
    -m 1G \
    -smp 3 \
    -drive file=fs.img,if=none,format=raw,id=x0 \
    -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
    -kernel $^

.PHONY: clean
clean:
	rm -rf target/
