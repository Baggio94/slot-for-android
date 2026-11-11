#!/usr/bin/env python3
"""Writes the bench's input-lag test ROM (Game Boy, no commercial content).

The whole screen is white, and turns black on the very frame the game sees
A held: each vblank it reads the pad and sets the background palette (BGP)
to all-white or all-black. The picture then only changes when A changes, so
a screen recording shows exactly when each press reached the screen.

usage: lagrom.py OUT.gb
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "gb-link-test"))
from asm import assemble, rom, JRNZ, JRZ, JR  # noqa: E402

main = assemble([
    0xF3,                    # di
    0x31, 0xFE, 0xDF,        # ld sp, $DFFE
    ('label', 'vbl'),
    0xF0, 0x44,              # ldh a, (LY)
    0xFE, 0x90,              # cp 144
    ('jr', JRNZ, 'vbl'),     # wait for the start of vblank
    0x3E, 0x10,              # ld a, $10       select the action buttons
    0xE0, 0x00,              # ldh (P1), a
    0xF0, 0x00, 0xF0, 0x00, 0xF0, 0x00, 0xF0, 0x00,  # ldh a, (P1) x4 (settle)
    0xCB, 0x47,              # bit 0, a        A: 0 = pressed
    0x3E, 0x00,              # ld a, $00       all white
    ('jr', JRNZ, 'set'),
    0x3E, 0xFF,              # ld a, $FF       all black
    ('label', 'set'),
    0xE0, 0x47,              # ldh (BGP), a
    0x3E, 0x30,              # ld a, $30
    0xE0, 0x00,              # ldh (P1), a     deselect
    ('label', 'out'),
    0xF0, 0x44,              # ldh a, (LY)
    0xFE, 0x90,              # cp 144
    ('jr', JRZ, 'out'),      # leave line 144 before waiting again
    ('jr', JR, 'vbl'),
])

with open(sys.argv[1], "wb") as f:
    f.write(rom(main, "SLOTLAGTEST"))
