#!/usr/bin/env python3
"""Prints the bench's default pad script: the same for every game, so runs
are comparable. It waits for the boot, presses Start and A to get through
title screens, then walks and presses buttons in a fixed pattern.

usage: input_script.py FRAMES > input.txt     ("FRAME MASK" lines)
"""
import sys

B, Y, SELECT, START, UP, DOWN, LEFT, RIGHT, A, X, L, R = (1 << i for i in range(12))
frames = int(sys.argv[1]) if len(sys.argv) > 1 else 3600
events = []
t = 240
for _ in range(4):  # title screens and menus
    events += [(t, START), (t + 6, 0), (t + 60, A), (t + 66, 0)]
    t += 120
pattern = [RIGHT, RIGHT | B, RIGHT | A, LEFT, UP, DOWN | Y, A, RIGHT | X, L, R, B, 0]
i = 0
while t < frames:
    events += [(t, pattern[i % len(pattern)]), (t + 20 + (i * 7) % 25, 0)]
    t += 50 + (i * 13) % 40
    i += 1
print("# frame mask (RetroPad bits)")
for frame, mask in events:
    if frame < frames:
        print(frame, mask)
