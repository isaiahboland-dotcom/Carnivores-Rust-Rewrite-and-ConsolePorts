    .section .rodata
    .balign 64
    .global c2_sample_shader
c2_sample_shader:
    .incbin "terrain.gsh"

    .balign 64
    .global c2_menu_shader
c2_menu_shader:
    .incbin "menu.gsh"
