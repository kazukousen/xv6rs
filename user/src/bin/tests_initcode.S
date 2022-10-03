.globl start
start:
  la a0, init
  la a1, argv
  li a7, 7
  ecall

init:
  .string "/tests\0"

.p2align 2
argv:
  .long init
  .long 0

