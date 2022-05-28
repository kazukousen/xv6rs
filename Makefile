CARGO_TARGET = riscv64imac-unknown-none-elf
TARGET = target/$(CARGO_TARGET)/debug
ifdef CARGO_RELEASE
	RELEASE = --release
	TARGET = target/$(CARGO_TARGET)/release
endif
CARGO ?= cargo
CARGO_BUILD = $(CARGO) build --frozen $(RELEASE) --target $(CARGO_TARGET)
CARGO_TEST = $(CARGO) test --frozen $(RELEASE) --target $(CARGO_TARGET)

KERNEL_TARGET_BIN = $(TARGET)/xv6rs-kernel

$(KERNEL_TARGET_BIN): fetch
	RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_BUILD) -p xv6rs-kernel

.PHONY: fetch
fetch:
	$(CARGO) fetch --locked

.PHONY: build
build: $(KERNEL_TARGET_BIN)

QEMU ?= qemu-system-riscv64
QEMUOPTS = -M virt \
    -bios none \
    -nographic \
    -m 1G \
    -smp 3
QEMUOPTS += -drive file=fs.img,if=none,format=raw,id=x0
QEMUOPTS += -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0

.PHONY: qemu
qemu: build
	$(QEMU) $(QEMUOPTS) -kernel $(KERNEL_TARGET_BIN)

.PHONY: test
test: fetch
	$(eval TEST_LIB := $(shell RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_TEST) -p xv6rs-kernel --lib --no-run --message-format=json \
						| jq -r 'select(.profile.test == true) | .filenames[0]'))
	$(QEMU) $(QEMUOPTS) -kernel $(TEST_LIB)

.PHONY: clean
clean:
	rm -rf target/
