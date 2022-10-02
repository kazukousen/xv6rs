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
	RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_BUILD) -p xv6rs-kernel --bin xv6rs-kernel

$(USER_TARGET_LIB): fetch
	RUSTFLAGS="--C link-arg=-Tuser/user.ld" $(CARGO_BUILD) -p xv6rs-user

.PHONY: fetch
fetch:
	$(CARGO) fetch

.PHONY: build
build: $(KERNEL_TARGET_BIN) $(USER_TARGET_LIB)


FWDPORT = $(shell expr `id -u` % 5000 + 25999)
SERVERPORT = $(shell expr `id -u` % 5000 + 25099)

QEMU ?= qemu-system-riscv64
QEMUOPTS = -M virt \
    -bios none \
    -nographic \
    -m 1G \
    -smp 3
QEMUOPTS += -drive file=fs.img,if=none,format=raw,id=x0
QEMUOPTS += -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
QEMUOPTS += -netdev user,id=net0,hostfwd=udp::$(FWDPORT)-:2000 -object filter-dump,id=net0,netdev=net0,file=packets.pcap
QEMUOPTS += -device e1000,netdev=net0,bus=pcie.0

.PHONY: qemu
qemu: build fs.img
	E1000_DEBUG=tx,txerr,rx,rxerr,general $(QEMU) $(QEMUOPTS) -kernel $(KERNEL_TARGET_BIN)

# RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" cargo test --frozen --release --target riscv64imac-unknown-none-elf -p xv6rs-kernel --lib --no-run
.PHONY: test
test: fetch fs.img
	@echo "building the test harness (rustc --test) artifact of kernel/lib.rs ..."
	$(eval KERNEL_LIB_TEST := $(shell RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_TEST) -p xv6rs-kernel --lib --no-run --message-format=json \
						| jq -r 'select(.profile.test == true) | .executable'))
	@echo "done $(KERNEL_LIB_TEST)"
	@echo "executing the artifact on qemu ..."
	$(QEMU) $(QEMUOPTS) -kernel $(KERNEL_LIB_TEST)

UPROGS=\
	user/_forktest\
	user/_grep\
	user/_kill\
	user/_ln\
	user/_mkdir\
	user/_rm\
	user/_sh\
	user/_stressfs\
	user/_usertests\
	user/_grind\
	user/_wc\
	user/_zombie\
	$(shell RUSTFLAGS="--C link-arg=-Tuser/user.ld" $(CARGO_BUILD) -p xv6rs-user --message-format=json \
						| jq -r 'select(.message == null) | select(.target.kind[0] == "bin") | .executable')\
	$(shell RUSTFLAGS="--C link-arg=-Tuser/user.ld" $(CARGO_TEST) -p xv6rs-user --no-run --message-format=json \
						| jq -r 'select(.profile.test == true) | .executable' | xargs -I{} sh -c 'b={}; ln -s "$${b}" "$${b%-*}-test"; echo "$${b%-*}-test"')\

$(UPROGS): $(USER_TARGET_LIB)

$(MKFS_TARGET_BIN): fetch
	$(CARGO) build --frozen $(RELEASE) --target $(CARGO_MKFS_TARGET) -p xv6rs-mkfs

fs.img: $(MKFS_TARGET_BIN) $(UPROGS) README.md
	$(MKFS_TARGET_BIN) $@ README.md $(UPROGS)

.PHONY: clean
clean:
	rm -rf target/
	rm -f fs.img

py-udp-server:
	python3 tools/udp-server.py $(SERVERPORT)

py-udp-ping:
	python3 tools/udp-ping.py $(FWDPORT)
