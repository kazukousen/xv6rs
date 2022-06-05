OUTPUT_ARCH( "risc_v" )
ENTRY( _start )

PHDRS
{
  text PT_LOAD;
  rodata PT_LOAD;
  data PT_LOAD;
  bss PT_LOAD;
}

SECTIONS
{
  .text : {
    *(.text._start)
    *(.text .text.*)
  }

  .rodata : {
    *(.rodata .rodata.*)
  }

  .data : {
    *(.sdata .sdata.*) *(.data .data.*)
  }

  .bss : {
    *(.sbss .sbss.*) *(.bss .bss.*)
  }
}

