"""Cut the symbol fonts the app bundles (app/assets/fonts/) down to the characters it draws.

Windows draws the app's icons and symbols with Segoe UI Symbol; elsewhere there's no font
everyone has, and egui can't draw a variable font's glyphs (Noto Emoji ships only as one). So
the app embeds small static subsets of Noto Emoji, DejaVu Sans, DejaVu Sans Mono, Noto Sans
Symbols, Noto Sans Math, Noto Sans Symbols 2 and Noto Sans CJK JP (only the full-width forms the
app uses, such as ＋, which a system's Japanese font may lack), covering every symbol in the
source and the game data. Rerun this
when the app gains a symbol (agent::tests::tool_icons_render fails on a missing one) or after a
knowledge update.

Needs fontTools (pip install fonttools). Run from the repo root with the source fonts, in the
order they should be tried (emoji first, as Segoe UI Symbol draws them):
  python -I tools/subset_fonts.py NotoEmoji.ttf DejaVuSans.ttf DejaVuSansMono.ttf \
      NotoSansSymbols.ttf NotoSansMath-Regular.otf NotoSansSymbols2-Regular.otf \
      NotoSansCJKjp-Regular.otf
Each font gets the symbols the ones before it lack, written as <name>-umaLord.ttf, and a
variable font is pinned to its Regular weight. Noto comes from github.com/notofonts and
github.com/googlefonts/noto-emoji, DejaVu from github.com/dejavu-fonts/dejavu-fonts. The app
loads them in the same order (theme::BUNDLED).
"""

import pathlib
import sys

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

OUT = pathlib.Path("app/assets/fonts")
SOURCES = [*pathlib.Path("app/src").rglob("*.rs"), *pathlib.Path("knowledge/data").glob("*.json"), *pathlib.Path("knowledge").glob("*.md")]


# Glyphs libraries draw that the app's own source doesn't name: egui_commonmark's code-block copy
# button (🗐, then ✔ once copied).
LIBRARY_SYMBOLS = "🗐✔"


def wanted() -> set[int]:
    """Symbols in the source and data, and LIBRARY_SYMBOLS: everything from General Punctuation
    up, minus CJK, Hangul, half-width katakana, variation selectors and joiners, which other fonts
    cover."""
    skip = [(0x3000, 0x9FFF), (0xAC00, 0xD7AF), (0xFF61, 0xFF9F), (0xFE00, 0xFE0F), (0xE000, 0xF8FF), (0xF0000, 0x10FFFF)]
    out = set(map(ord, LIBRARY_SYMBOLS))
    for path in SOURCES:
        for ch in path.read_text(encoding="utf-8"):
            o = ord(ch)
            if o >= 0x2000 and o not in (0x200D, 0x2028, 0x2029) and not any(a <= o <= b for a, b in skip):
                out.add(o)
    return out


def cut(font: TTFont, codepoints: set[int], name: str) -> set[int]:
    have = codepoints & set(font.getBestCmap())
    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.name_IDs = ["*"]
    sub = subset.Subsetter(options)
    sub.populate(unicodes=sorted(have))
    sub.subset(font)
    font.save(OUT / name)
    print(f"{name}: {len(have)} glyphs, {(OUT / name).stat().st_size} bytes")
    return have


def main() -> None:
    left = wanted()
    OUT.mkdir(parents=True, exist_ok=True)
    for path in map(pathlib.Path, sys.argv[1:]):
        font = TTFont(path)
        if "fvar" in font:
            font = instancer.instantiateVariableFont(font, {"wght": 400})
        name = path.stem.split("[")[0].removesuffix("-Regular") + "-umaLord.ttf"
        left -= cut(font, left, name)
    if left:
        print("Not covered (egui's own fonts or the system's may have them):", "".join(map(chr, sorted(left))))


if __name__ == "__main__":
    main()
