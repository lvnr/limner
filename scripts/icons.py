#!/usr/bin/env python3
"""Turns the icon drawings in art/icons/*.svg into godot/ui/icon_art.gd.

The drawings are black (and a few greys) on a 48 x 48 grid. In the generated file
black becomes the placeholder {c} for the colour the icon is drawn in, a grey becomes
that colour at a matching opacity (lighter grey, fainter), and the size becomes {px}.
Godot does not pack loose .svg sources into a built app, so the drawings travel as
text inside a script. Run it again after changing a drawing:

    python3 scripts/icons.py
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "art" / "icons"
TARGET = ROOT / "godot" / "ui" / "icon_art.gd"

COLOUR = re.compile(r'(fill|stroke)="#([0-9a-fA-F]{6})"')


def tint(match: re.Match) -> str:
    attribute, value = match.group(1), match.group(2)
    grey = int(value[0:2], 16) / 255.0
    if grey == 0.0:
        return f'{attribute}="{{c}}"'
    return f'{attribute}="{{c}}" {attribute}-opacity="{1.0 - grey:.3f}"'


def convert(svg: str) -> str:
    svg = re.sub(r"<title>.*?</title>", "", svg)
    svg = svg.replace('width="48" height="48"', 'width="{px}" height="{px}"', 1)
    svg = COLOUR.sub(tint, svg)
    if "'" in svg:
        raise ValueError("a drawing holds a single quote, which the generated script cannot")
    return svg.strip()


def main() -> None:
    lines = [
        "class_name IconArt",
        "## The filled icon drawings, made by scripts/icons.py from art/icons/*.svg: do not",
        "## edit by hand. {c} is the colour and {px} the size; see `Icons.texture`.",
        "",
        "const ART := {",
    ]
    for path in sorted(SOURCE.glob("*.svg")):
        svg = convert(path.read_text(encoding="utf-8"))
        lines.append(f"\t\"{path.stem}\": '{svg}',")
    lines.append("}")
    TARGET.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"{TARGET.relative_to(ROOT)}: {len(lines) - 6} icons")


if __name__ == "__main__":
    main()
