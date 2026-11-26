#!/usr/bin/env python3
"""The PICO-8 shelf's Default wallpaper mask (assets/wallpapers/PICO8.png).

A 2048x2048 one-channel mask like the other shelves': 255 where the line
is drawn, 0 on the ground. PICO-8's own look: 8x8 sprites (a heart, a cart,
a star, the O and X buttons, a ghost, a key, a coin) whose pixels are
square dots, scattered on a staggered grid.

usage: python3 tools/pico8-mould/wallpaper.py assets/wallpapers/PICO8.png
"""
import random
import sys

from PIL import Image, ImageDraw

SIZE = 2048
DOT = 7      # one sprite pixel, in mask pixels
GAP = 2      # between two dots
STEP = 150   # the grid's pitch

SPRITES = {
    "heart": [
        "........",
        ".##..##.",
        "########",
        "########",
        ".######.",
        "..####..",
        "...##...",
        "........",
    ],
    "cart": [
        "#######.",
        "#.....##",
        "#.###..#",
        "#.###..#",
        "#.###..#",
        "#......#",
        "#.#.#.##",
        "#######.",
    ],
    "star": [
        "...#....",
        "...#....",
        "..###...",
        "#######.",
        ".#####..",
        "..###...",
        ".##.##..",
        "##...##.",
    ],
    "o": [
        "..####..",
        ".#....#.",
        "#......#",
        "#......#",
        "#......#",
        "#......#",
        ".#....#.",
        "..####..",
    ],
    "x": [
        "#......#",
        ".#....#.",
        "..#..#..",
        "...##...",
        "...##...",
        "..#..#..",
        ".#....#.",
        "#......#",
    ],
    "ghost": [
        "..####..",
        ".######.",
        "##.##.##",
        "########",
        "########",
        "########",
        "#.##.##.",
        "........",
    ],
    "key": [
        ".###....",
        "#...#...",
        "#...#...",
        ".###....",
        "..#.....",
        "..###...",
        "..#.....",
        "..##....",
    ],
    "coin": [
        "..####..",
        ".#....#.",
        "#..##..#",
        "#.#....#",
        "#.#....#",
        "#..##..#",
        ".#....#.",
        "..####..",
    ],
}


def sprite(draw, name, x0, y0):
    for j, row in enumerate(SPRITES[name]):
        for i, c in enumerate(row):
            if c == "#":
                x = x0 + i * (DOT + GAP)
                y = y0 + j * (DOT + GAP)
                draw.rectangle([x, y, x + DOT - 1, y + DOT - 1], fill=255)


def main(out):
    image = Image.new("L", (SIZE, SIZE), 0)
    draw = ImageDraw.Draw(image)
    names = list(SPRITES)
    rng = random.Random(8)
    side = 8 * (DOT + GAP) - GAP
    for row in range(SIZE // STEP + 1):
        for col in range(SIZE // STEP + 1):
            # Every other row half a pitch over; a little jitter so the grid
            # does not read as a grid.
            x = col * STEP + (STEP // 2 if row % 2 else 0) + rng.randint(-12, 12)
            y = row * STEP + rng.randint(-12, 12)
            name = names[(row * 3 + col * 5) % len(names)]
            sprite(draw, name, x - side // 2 + STEP // 4, y - side // 2 + STEP // 4)
    image.save(out, optimize=True)


if __name__ == "__main__":
    main(sys.argv[1])
