"""Banner do Peabiru: placa de pergaminho com o emblema, a rota e o nome.

Uso: python3 -I banner.py   (gera peabiru-banner.svg e g_banner.txt)

O nome e desenhado por uma fonte bitmap definida aqui (minusculas, haste
de 3 px, altura x de 15). Nada de texto no SVG: so retangulos.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from pix import blank, dump, to_svg, verify  # noqa: E402
import emblem as EM  # noqa: E402

# ------------------------------------------------------------ fonte bitmap
# 'X' = tinta; 'I' = pingo do i (vira um no de rota turquesa).
# Cada glifo comeca na linha 0 da altura x; ASC e DESC dizem quantas linhas
# ele ocupa acima e abaixo dela.
X15 = 15
GLYPHS = {
    "p": (0, 5, [
        "XXXXXXXXX..",
        "XXXXXXXXXX.",
        "XXXXXXXXXXX",
    ] + ["XXX.....XXX"] * 9 + [
        "XXXXXXXXXXX",
        "XXXXXXXXXX.",
        "XXXXXXXXX..",
    ] + ["XXX........"] * 5),
    "e": (0, 0, [
        "..XXXXXXX..",
        ".XXXXXXXXX.",
        "XXXXXXXXXXX",
        "XXX.....XXX",
        "XXX.....XXX",
        "XXX.....XXX",
        "XXXXXXXXXXX",
        "XXXXXXXXXXX",
        "XXXXXXXXXXX",
        "XXX........",
        "XXX........",
        "XXX........",
        "XXXXXXXXXXX",
        ".XXXXXXXXXX",
        "..XXXXXXXXX",
    ]),
    "a": (0, 0, [
        "..XXXXXXX..",
        ".XXXXXXXXX.",
        ".XXXXXXXXXX",
        "........XXX",
        "........XXX",
        "........XXX",
        "..XXXXXXXXX",
        ".XXXXXXXXXX",
        "XXXXXXXXXXX",
        "XXX.....XXX",
        "XXX.....XXX",
        "XXX.....XXX",
        "XXXXXXXXXXX",
        ".XXXXXXXXXX",
        "..XXXXXXXXX",
    ]),
    "b": (5, 0, ["XXX........"] * 5 + [
        "XXXXXXXXX..",
        "XXXXXXXXXX.",
        "XXXXXXXXXXX",
    ] + ["XXX.....XXX"] * 9 + [
        "XXXXXXXXXXX",
        "XXXXXXXXXX.",
        "XXXXXXXXX..",
    ]),
    "i": (5, 0, ["III", "III", "III", "...", "..."] + ["XXX"] * 15),
    "r": (0, 0, [
        "XXXXXXX.",
        "XXXXXXXX",
        "XXXXXXXX",
    ] + ["XXX....."] * 12),
    "u": (0, 0, ["XXX.....XXX"] * 12 + [
        "XXXXXXXXXXX",
        ".XXXXXXXXXX",
        "..XXXXXXXXX",
    ]),
}
TRACK = 4                     # 1 px de sombra + 3 px livres entre letras
KERN = {("i", "r"): 1,        # par reto com reto: abre mais 1 px
        ("r", "u"): -1}       # o braco do r ja ocupa o alto do vao


def glyph_w(c):
    return len(GLYPHS[c][2][0])


def advance(a, b):
    return glyph_w(a) + TRACK + KERN.get((a, b), 0)


def text_width(s):
    return sum(advance(a, b) for a, b in zip(s, s[1:])) + glyph_w(s[-1])


def draw_text(g, s, x0, ytop, ink="K", shadow="T", dot="Q"):
    """ytop = linha do topo da altura x."""
    for ox, oy, layer in ((1, 1, "sh"), (0, 0, "ink")):
        x = x0
        for i, c in enumerate(s):
            asc, _desc, rows = GLYPHS[c]
            for dy, row in enumerate(rows):
                for dx, ch in enumerate(row):
                    if ch == ".":
                        continue
                    yy, xx = ytop - asc + dy + oy, x + dx + ox
                    if layer == "sh":
                        if g[yy][xx] in ("P", "e"):
                            g[yy][xx] = shadow
                    else:
                        g[yy][xx] = dot if ch == "I" else ink
            if i + 1 < len(s):
                x += advance(c, s[i + 1])


# ----------------------------------------------------------------- placa

H = 44
EX, EY = 6, 6          # emblema 32x32
WORD = "peabiru"
ROUTE_GAP = 34         # do aro do emblema ate o p
XTOP = 14              # topo da altura x (centro da altura x = centro do emblema)
ZIG = ["O....O", ".O..O.", "..OO.."]  # ziguezague laranja sobre terracota, periodo 6
ROUTE = "RR..RR..QQQ..RR..RR..BBB..RR"


def banner():
    tw = text_width(WORD)
    wx = EX + 32 + ROUTE_GAP
    W = wx + tw + 1 + (EX - 0)  # margem direita igual a esquerda (conta a sombra)
    g = blank(W, H, "P")
    # contorno escuro com cantos chanfrados
    for x in range(W):
        g[0][x] = g[H - 1][x] = "K"
    for y in range(H):
        g[y][0] = g[y][W - 1] = "K"
    for cx, cy, sx, sy in ((0, 0, 1, 1), (W - 1, 0, -1, 1), (0, H - 1, 1, -1), (W - 1, H - 1, -1, -1)):
        g[cy][cx] = g[cy][cx + sx] = g[cy + sy][cx] = "."
        g[cy + sy][cx + sx] = "K"
    # faixas terracota com ziguezague laranja em cima e embaixo
    mid = W // 2
    for y0, rows in ((2, ZIG), (H - 5, ZIG[::-1])):
        for dy, pat in enumerate(rows):
            for x in range(2, W - 2):
                g[y0 + dy][x] = "O" if pat[(x - mid) % len(pat)] == "O" else "T"
    # emblema
    emb = EM.emblem()
    for y in range(32):
        for x in range(32):
            if emb[y][x] != ".":
                g[EY + y][EX + x] = emb[y][x]
    # rota: tracejado vermelho saindo da marca leste do aro, com dois saltos
    # (turquesa e azul pena) antes de chegar ao nome
    ry = XTOP + 7
    x0, x1 = EX + 32 + 2, wx - 3
    pat = ROUTE
    start = x0 + (x1 - x0 - len(pat)) // 2
    for i, ch in enumerate(pat):
        if ch == ".":
            continue
        for yy in ((ry,) if ch == "R" else (ry - 1, ry, ry + 1)):
            g[yy][start + i] = ch
    draw_text(g, WORD, wx, XTOP)
    return g


TITLE = "Peabiru: emblema da bússola e o nome em pixel art"
DESC = ("Banner em pixel art: placa de pergaminho com o emblema da bússola e da pegada de onça, "
        "uma rota tracejada com dois saltos e o nome peabiru desenhado em fonte bitmap.")


def main():
    g = banner()
    open(os.path.join(HERE, "g_banner.txt"), "w").write(dump(g))
    svg = to_svg(g, TITLE, DESC, scale=8, base=("P", 1, 1, len(g[0]) - 2, len(g) - 2))
    open(os.path.join(HERE, "peabiru-banner.svg"), "w", encoding="utf-8").write(svg)
    bad = verify(g, svg)
    print("peabiru-banner.svg", len(svg.encode()), "bytes;", len(g[0]), "x", len(g),
          "(%.2f:1)" % (len(g[0]) / len(g)), "verificacao:", "ok" if not bad else bad[:10])


if __name__ == "__main__":
    main()
