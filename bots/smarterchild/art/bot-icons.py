"""The bots' Buddy Icons (BugBot, TheTranslator) as pixel grids: `python bot-icons.py OUT_DIR`
writes 64x64 PNGs (3x, transparent) and previews on HIM's gray. Needs Pillow."""
import sys
from PIL import Image
PAL = {"#": (0, 0, 0), "W": (255, 255, 255), "o": (205, 205, 205), "=": (154, 154, 154),
       "R": (255, 0, 0), "B": (40, 100, 215), "G": (40, 165, 70), "Y": (255, 205, 0)}

BUGBOT = [
    "...RR.........RR...",
    "...RR.........RR...",
    ".....#.......#.....",
    "......#.....#......",
    ".....#########.....",
    ".....#Wooooo=#.....",
    ".....#WRRoRR=#.....",
    ".....#WRRoRR=#.....",
    ".....#Wooooo=#.....",
    ".....#Wo#o#o=#.....",
    ".....#########.....",
    ".....##RR#RR##.....",
    "#...#RRRR#RRRR#...#",
    ".#.#RR##R#R##RR#.#.",
    "..##RR##R#R##RR##..",
    "...#RRRRR#RRRRR#...",
    ".###RR##R#R##RR###.",
    "#..#RRRRR#RRRRR#..#",
    ".....#RRR#RRR#.....",
    "......#######......",
]

TRANSLATOR = [
    "#####....###....#####",
    "#WWW#....#R#....#WWW#",
    "#RRR#.#########.#BBB#",
    "#WWW#.#Wooooo=#.#WWW#",
    "#RRW#.#Wo#o#o=#.#WBB#",
    "#WWW#.#Wooooo=#.#WWW#",
    "#####.#W#ooo#=#.#####",
    "....#.#Wo###o=#.#....",
    ".....##Wooooo=##.....",
    "...###############...",
    "...######BBB######...",
    "...#####BGGBB#####...",
    "...#o##BBGGGBB##=#...",
    "...#o##BBBGBBB##=#...",
    "...#o###BBGGB###=#...",
    "...######BBB######...",
    "...###############...",
    ".....###########.....",
    ".....##.......##.....",
    "....###.......###....",
]

def make(grid, out, preview):
    w = len(grid[0])
    assert all(len(r) == w for r in grid), [len(r) for r in grid]
    h = len(grid)
    small = Image.new("RGBA", (w, h))
    for y, row in enumerate(grid):
        for x, c in enumerate(row):
            small.putpixel((x, y), (0, 0, 0, 0) if c == "." else PAL[c] + (255,))
    k = min(64 // w, 64 // h)
    icon = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
    icon.paste(small.resize((w * k, h * k), Image.NEAREST), ((64 - w * k) // 2, (64 - h * k) // 2))
    icon.save(out, optimize=True)
    bg = Image.new("RGBA", (64, 64), (233, 233, 233, 255))   # HIM's gray behind icons
    bg.alpha_composite(icon)
    bg.resize((256, 256), Image.NEAREST).save(preview)

make(BUGBOT, sys.argv[1] + "/bugbot-icon.png", sys.argv[1] + "/bugbot-preview.png")
make(TRANSLATOR, sys.argv[1] + "/translator-icon.png", sys.argv[1] + "/translator-preview.png")
