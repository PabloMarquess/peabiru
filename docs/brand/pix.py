"""Utilidades de pixel art do Peabiru: paleta, grade, SVG, PNG e verificacao.

Cada caractere da grade e uma cor. '.' e transparente.
"""
import math
import os

PAL = {
    "K": "#2b1a10",  # contorno
    "M": "#5a341d",  # marrom
    "R": "#8e1f1a",  # vermelho escuro (tracejado do trace)
    "T": "#b33a26",  # terracota
    "O": "#e8742a",  # laranja
    "G": "#f4b63a",  # dourado
    "S": "#ffd36b",  # sol
    "W": "#fff1c9",  # miolo do sol
    "P": "#f1dfb4",  # pergaminho
    "C": "#d9a35a",  # ocre
    "E": "#b9783a",  # terra
    "B": "#2457a6",  # azul pena
    "Q": "#1d8c8c",  # turquesa
    "V": "#4f7d2b",  # verde folha
    "D": "#2f5220",  # verde escuro
    "e": "#e4c88f",  # pergaminho sombreado
    "r": "#c9532c",  # raio sobre terracota
    "l": "#f0923a",  # laranja claro
    "g": "#fac64f",  # dourado claro
    "v": "#3d6625",  # verde de transicao
}


def blank(w, h, c="."):
    return [[c] * w for _ in range(h)]


def dump(g):
    return "\n".join("".join(r) for r in g) + "\n"


def stamp(g, sprite, x0, y0, remap=None, mask=None):
    for dy, row in enumerate(sprite):
        for dx, ch in enumerate(row):
            if ch == ".":
                continue
            x, y = x0 + dx, y0 + dy
            if 0 <= y < len(g) and 0 <= x < len(g[0]):
                if mask is None or mask[y][x]:
                    g[y][x] = (remap or {}).get(ch, ch)


def rects(g):
    """Corridas horizontais da mesma cor; corridas iguais em linhas
    seguidas viram um retangulo mais alto. Devolve {cor: [(x,y,w,h)]}."""
    h, w = len(g), len(g[0])
    runs = {}
    for y, row in enumerate(g):
        x = 0
        while x < w:
            ch = row[x]
            x1 = x
            while x1 + 1 < w and row[x1 + 1] == ch:
                x1 += 1
            if ch != ".":
                runs.setdefault((x, x1 - x + 1, ch), []).append(y)
            x = x1 + 1
    out = {}
    for (x, n, ch), ys in runs.items():
        ys.sort()
        start = prev = ys[0]
        for y in ys[1:] + [None]:
            if y is not None and y == prev + 1:
                prev = y
                continue
            out.setdefault(ch, []).append((x, start, n, prev - start + 1))
            if y is not None:
                start = prev = y
    return out


def to_svg(g, title, desc, scale=16, base=None):
    """base = (cor, x, y, w, h): um retangulo de fundo pintado primeiro;
    pixels dessa cor dentro dele nao precisam de rect proprio."""
    h, w = len(g), len(g[0])
    gg = [row[:] for row in g]
    body = []
    if base:
        bc, bx, by, bw, bh = base
        body.append(f'<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" fill="{PAL[bc]}"/>')
        for y in range(by, by + bh):
            for x in range(bx, bx + bw):
                if gg[y][x] == bc:
                    gg[y][x] = "."
    rs = rects(gg)
    for ch in sorted(rs, key=lambda c: -len(rs[c])):
        items = "".join(
            f'<rect x="{x}" y="{y}" width="{ww}" height="{hh}"/>' for x, y, ww, hh in rs[ch]
        )
        body.append(f'<g fill="{PAL[ch]}">{items}</g>')
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" '
        f'width="{w * scale}" height="{h * scale}" shape-rendering="crispEdges" '
        f'role="img" aria-labelledby="t d">'
        f'<title id="t">{title}</title><desc id="d">{desc}</desc>'
        + "".join(body)
        + "</svg>\n"
    )


def verify(g, svg_text):
    """Rasteriza os rects do SVG de volta e compara com a grade."""
    import re

    h, w = len(g), len(g[0])
    back = blank(w, h)
    inv = {v: k for k, v in PAL.items()}
    pos = 0
    # fundo opcional antes dos grupos
    for m in re.finditer(r'<rect x="(\d+)" y="(\d+)" width="(\d+)" height="(\d+)" fill="(#[0-9a-f]{6})"/>', svg_text):
        x, y, ww, hh, col = int(m[1]), int(m[2]), int(m[3]), int(m[4]), m[5]
        for yy in range(y, y + hh):
            for xx in range(x, x + ww):
                back[yy][xx] = inv[col]
    for gm in re.finditer(r'<g fill="(#[0-9a-f]{6})">(.*?)</g>', svg_text):
        col = inv[gm[1]]
        for m in re.finditer(r'<rect x="(\d+)" y="(\d+)" width="(\d+)" height="(\d+)"/>', gm[2]):
            x, y, ww, hh = int(m[1]), int(m[2]), int(m[3]), int(m[4])
            for yy in range(y, y + hh):
                for xx in range(x, x + ww):
                    back[yy][xx] = col
    bad = [(x, y) for y in range(h) for x in range(w) if back[y][x] != g[y][x]]
    return bad


def hex2rgb(hx):
    hx = hx.lstrip("#")
    return tuple(int(hx[i:i + 2], 16) for i in (0, 2, 4))


def to_png(g, path, scale, bg="#ffffff", pad=0):
    """PNG exato a partir da grade: ampliacao por vizinho mais proximo
    (scale inteiro) ou reducao por media de area (scale = 1/k)."""
    from PIL import Image

    h, w = len(g), len(g[0])
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    px = im.load()
    for y in range(h):
        for x in range(w):
            if g[y][x] != ".":
                px[x, y] = hex2rgb(PAL[g[y][x]]) + (255,)
    if scale >= 1:
        im = im.resize((w * int(scale), h * int(scale)), Image.NEAREST)
    else:
        im = im.resize((max(1, round(w * scale)), max(1, round(h * scale))), Image.BOX)
    out = Image.new("RGB", (im.width + 2 * pad, im.height + 2 * pad), hex2rgb(bg))
    out.paste(im, (pad, pad), im)
    out.save(path)
    return out


def luminance(hx):
    def ch(c):
        c /= 255
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = hex2rgb(hx)
    return 0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)


def contrast(a, b):
    la, lb = luminance(a), luminance(b)
    hi, lo = max(la, lb), min(la, lb)
    return (hi + 0.05) / (lo + 0.05)
