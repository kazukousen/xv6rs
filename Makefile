CARGO_TARGET = riscv64imac-unknown-none-elf
CARGO_MKFS_TARGET ?= $(shell rustup show | sed -n 's/^Default host: \(.*\)/\1/p')
TARGET = target/$(CARGO_TARGET)/debug
MKFS_TARGET = target/$(CARGO_MKFS_TARGET)/debug
ifdef CARGO_RELEASE
	RELEASE = --release
	TARGET = target/$(CARGO_TARGET)/release
	MKFS_TARGET = target/$(CARGO_MKFS_TARGET)/release
endif
CARGO ?= cargo
CARGO_BUILD = $(CARGO) build --frozen $(RELEASE) --target $(CARGO_TARGET)
CARGO_TEST = $(CARGO) test --frozen $(RELEASE) --target $(CARGO_TARGET)

KERNEL_TARGET_BIN = $(TARGET)/xv6rs-kernel
MKFS_TARGET_BIN = $(MKFS_TARGET)/xv6rs-mkfs
USER_TARGET_LIB = $(TARGET)/xv6rs-user

$(KERNEL_TARGET_BIN): fetch
	RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_BUILD) -p xv6rs-kernel

$(USER_TARGET_LIB): fetch
	RUSTFLAGS="--C link-arg=-Tuser/user.ld" $(CARGO_BUILD) -p xv6rs-user

.PHONY: fetch
fetch:
	$(CARGO) fetch

.PHONY: build
build: $(KERNEL_TARGET_BIN) $(USER_TARGET_LIB)

QEMU ?= qemu-system-riscv64
QEMUOPTS = -M virt \
    -bios none \
    -nographic \
    -m 1G \
    -smp 3
QEMUOPTS += -drive file=fs.img,if=none,format=raw,id=x0
QEMUOPTS += -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0

.PHONY: qemu
qemu: build fs.img
	$(QEMU) $(QEMUOPTS) -kernel $(KERNEL_TARGET_BIN)

.PHONY: test
test: fetch fs.img
	$(eval TEST_LIB := $(shell RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_TEST) -p xv6rs-kernel --lib --no-run --message-format=json \
						| jq -r 'select(.profile.test == true) | .filenames[0]'))
	$(QEMU) $(QEMUOPTS) -kernel $(TEST_LIB)

UPROGS=\
	$(TARGET)/cat\
	user/_echo\
	user/_forktest\
	user/_grep\
	user/_init\
	user/_kill\
	user/_ln\
	user/_ls\
	user/_mkdir\
	user/_rm\
	user/_sh\
	user/_stressfs\
	user/_usertests\
	user/_grind\
	user/_wc\
	user/_zombie\
	$(TARGET)/helloworld\
	$(TARGET)/exit42\

$(MKFS_TARGET_BIN): fetch
	$(CARGO) build --frozen $(RELEASE) --target $(CARGO_MKFS_TARGET) -p xv6rs-mkfs

fs.img: $(MKFS_TARGET_BIN) $(UPROGS) README.md
	$(MKFS_TARGET_BIN) $@ $(UPROGS) README.md

.PHONY: clean
clean:
	rm -rf target/
	rm -f fs.img
