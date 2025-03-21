CARGO_TARGET = riscv64imac-unknown-none-elf
CARGO_MKFS_TARGET ?= $(shell rustup show | sed -n 's/^Default host: \(.*\)/\1/p')
RELEASE = --release
TARGET = target/$(CARGO_TARGET)/release
MKFS_TARGET = target/$(CARGO_MKFS_TARGET)/release
CARGO ?= cargo +nightly
CARGO_BUILD = $(CARGO) build --frozen $(RELEASE) --target $(CARGO_TARGET)
CARGO_TEST = $(CARGO) test --frozen $(RELEASE) --target $(CARGO_TARGET)

KERNEL_TARGET_BIN = $(TARGET)/xv6rs-kernel
MKFS_TARGET_BIN = $(MKFS_TARGET)/xv6rs-mkfs

# fetch dependencies from the network
Cargo.lock: Cargo.toml
	$(CARGO) fetch

KERNEL_SRC := $(shell find kernel -type f)
USER_SRC := $(shell find user -type f)
MKFS_SRC := $(shell find mkfs -type f)

# build the kernel binary
$(KERNEL_TARGET_BIN): Cargo.lock $(KERNEL_SRC)
	RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_BUILD) -p xv6rs-kernel --bin xv6rs-kernel

USER_PROGRAMS=\
	$(shell find user/src/bin -type f -name '*.rs' | sed 's%user/src/bin/\(.*\).rs%$(TARGET)/\1%g')

$(USER_PROGRAMS): Cargo.lock $(USER_SRC)
	RUSTFLAGS="--C link-arg=-Tuser/user.ld" $(CARGO_BUILD) -p xv6rs-user

.PHONY: build
build: $(KERNEL_TARGET_BIN) $(USER_PROGRAMS)

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

$(MKFS_TARGET_BIN): Cargo.lock $(MKFS_SRC)
	$(CARGO) build --frozen $(RELEASE) --target $(CARGO_MKFS_TARGET) -p xv6rs-mkfs

fs.img: $(MKFS_TARGET_BIN) $(UPROGS) $(USER_PROGRAMS) README.md
	$(MKFS_TARGET_BIN) $@ README.md $(UPROGS) $(USER_PROGRAMS)

FWDPORT = $(shell expr `id -u` % 5000 + 25999)
SERVERPORT = $(shell expr `id -u` % 5000 + 25099)

QEMU ?= qemu-system-riscv64
QEMU_OPTS_BASE = -M virt \
    -bios none \
    -nographic \
    -m 1G \
    -smp 3
QEMU_OPTS_BASE += -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
QEMU_OPTS_BASE += -netdev user,id=net0,hostfwd=udp::$(FWDPORT)-:2000 -object filter-dump,id=net0,netdev=net0,file=packets.pcap
QEMU_OPTS_BASE += -device e1000,netdev=net0,bus=pcie.0
QEMU_OPTS := $(QEMU_OPTS_BASE) -drive file=fs.img,if=none,format=raw,id=x0

.PHONY: qemu
qemu: build fs.img
	E1000_DEBUG=tx,txerr,rx,rxerr,general $(QEMU) $(QEMU_OPTS) -kernel $(KERNEL_TARGET_BIN)

# RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" cargo test --frozen --release --target riscv64imac-unknown-none-elf -p xv6rs-kernel --lib --no-run
.PHONY: test
test: $(MKFS_TARGET_BIN) $(USER_PROGRAMS)
	@echo "building the test harness (rustc --test) artifact of user/... ..."
	$(eval USER_LIB_TEST := $(shell RUSTFLAGS="--C link-arg=-Tuser/user.ld" $(CARGO_TEST) -p xv6rs-user --no-run --message-format=json \
						| jq -r 'select(.profile.test == true) | .executable' | xargs -I{} sh -c 'b={}; ln -s "$${b}" "$${b%-*}.test"; echo "$${b%-*}.test"'))
	@echo "done $(USER_LIB_TEST)"
	@echo "creating the file system ..."
	$(MKFS_TARGET_BIN) fs.test.img $(USER_LIB_TEST) $(USER_PROGRAMS)
	@echo "building the test harness (rustc --test) artifact of kernel/lib.rs ..."
	$(eval KERNEL_LIB_TEST := $(shell RUSTFLAGS="--C link-arg=-Tkernel/kernel.ld" $(CARGO_TEST) -p xv6rs-kernel --lib --no-run --message-format=json \
						| jq -r 'select(.profile.test == true) | .executable'))
	@echo "done $(KERNEL_LIB_TEST)"
	@echo "executing the artifact on qemu ..."
	$(eval QEMU_OPTS_TEST := $(QEMU_OPTS_BASE) -drive file=fs.test.img,if=none,format=raw,id=x0)
	$(QEMU) $(QEMU_OPTS_TEST) -kernel $(KERNEL_LIB_TEST)

user/src/bin/tests_initcode: user/src/bin/tests_initcode.S
	riscv64-unknown-elf-gcc -march=rv64g -nostdinc -c $@.S -o $@.o
	riscv64-unknown-elf-ld -z max-page-size=4096 -N -e start -Ttext 0 -o $@.out $@.o
	riscv64-unknown-elf-objcopy -S -O binary $@.out $@
	rm $@.out $@.o
	# od -t xC $@

.PHONY: clean
clean:
	rm -rf target/
	rm -f fs.img
	rm -f fs.test.img

py-udp-server:
	python3 tools/udp-server.py $(SERVERPORT)

py-udp-ping:
	python3 tools/udp-ping.py $(FWDPORT)
