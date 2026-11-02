"""Emblema do Peabiru: bussola redonda 32x32 com paisagem e rastro.

Uso: python3 -I emblem.py

A grade tem 32x32, entao escala por inteiro para 32, 64, 128 e 512.
Leitura de baixo para cima: a trilha de terra entra pela borda (ponto A),
leva o tracejado vermelho do trace ate a pegada de onca (onde o rastro
parou), e dali segue clara, sem borda, passando por um salto vazio (ainda
nao respondeu) ate o sol nascente atras da serra (ponto B).
"""
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from pix import blank, dump, stamp, to_svg, verify, to_png  # noqa: E402

N = 32
C0 = N / 2


def rad(x, y):
    return math.hypot(x + 0.5 - C0, y + 0.5 - C0)


def ang(x, y):
    """angulo de bussola em graus: 0 = norte, 90 = leste"""
    return math.degrees(math.atan2(x + 0.5 - C0, C0 - (y + 0.5))) % 360


def disc(r):
    return [[rad(x, y) < r for x in range(N)] for y in range(N)]


def edge(m):
    out = [[False] * N for _ in range(N)]
    for y in range(N):
        for x in range(N):
            if not m[y][x]:
                continue
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                xx, yy = x + dx, y + dy
                if not (0 <= xx < N and 0 <= yy < N) or not m[yy][xx]:
                    out[y][x] = True
                    break
    return out


R_OUT, R_IN = 16.0, 13.0
OUTER, INNER = disc(R_OUT), disc(R_IN)
OE, IE = edge(OUTER), edge(INNER)
INSIDE = [[INNER[y][x] and not IE[y][x] for x in range(N)] for y in range(N)]


def ring(g, marks=True):
    """Aro de bussola: contorno escuro por fora e por dentro, faixa
    terracota continua e marcas nos quatro pontos cardeais (N e S em
    turquesa, L e O em azul pena), cada uma com um dente para dentro."""
    for y in range(N):
        for x in range(N):
            if not OUTER[y][x]:
                g[y][x] = "."
            elif INSIDE[y][x]:
                continue
            elif OE[y][x] or IE[y][x]:
                g[y][x] = "K"
            else:
                g[y][x] = "T"
    if not marks:
        return
    for y in range(N):
        for x in range(N):
            if INSIDE[y][x] or not OUTER[y][x] or OE[y][x]:
                continue
            a = ang(x, y)
            for card, col in ((0, "Q"), (90, "B"), (180, "Q"), (270, "B")):
                d = min(abs(a - card), 360 - abs(a - card))
                if not IE[y][x] and d < 9.5:
                    g[y][x] = col
                if IE[y][x] and d < 4.5:
                    g[y][x] = col


# ------------------------------------------------------------------ cena

HZ = 11  # linha do horizonte

SKY = {4: "r", 5: "O", 6: "O", 7: "l", 8: "G", 9: "G", 10: "G"}

SUN = [
    "..SSSS..",
    ".SWWWWS.",
    "SWWWWWWS",
    "SWWWWWWS",
    "SWWWWWWS",
]
SUN_AT = (12, 6)
RAYS = [(15, 4), (16, 4), (11, 5), (12, 6), (20, 5), (19, 6)]

# serra: azul na sombra, turquesa na face voltada ao sol
SERRA_R = {8: (23, 24), 9: (22, 25), 10: (20, 26)}

# araucaria: tronco alto e copa chata com as pontas para cima (candelabro)
ARAUCARIA = [
    "V.V.V",
    "DDDDD",
    "..M..",
    "..M..",
    "..M..",
    "..M..",
]
ARAU_AT = (6, 8)

# pegada de onca: dedos em arco (o interno direito um pixel mais alto),
# externos mais baixos e abertos, almofada larga com base trilobada
PAW = [  # inclinada para cima e para a direita, no sentido do rastro (nada de rosto)
    "....XX.....",
    ".XX.XX.....",
    ".XX....XX..",
    "........XX.",
    "....XX..XX.",
    "..XXXXX....",
    ".XXXXXXX...",
    ".XXXXXXXX..",
    "..XXXXXXX..",
    "...XX.XX...",
]
PAW_AT = (12, 16)

# trecho percorrido, desenhado coluna a coluna: x -> (linha de cima, linha de baixo).
# Entra por baixo do aro a esquerda (ponto A) e chega na pegada. A ultima
# linha de cada coluna e a borda escura.
ROAD_DONE = {
    4: (18, 19), 5: (18, 21), 6: (19, 22), 7: (19, 22), 8: (20, 23),
    9: (20, 23), 10: (20, 23), 11: (21, 23), 12: (21, 23),
}
DASHES = [(5, 20), (6, 20), (9, 21), (10, 21)]  # tracejado vermelho do trace
# trecho pendente: terra clara e sem borda, da pegada ate a base do sol (ponto B)
ROAD_TODO = [(20, 15), (21, 15), (21, 12), (20, 11), (19, 11), (18, 11)]
HOP = [  # salto seguinte, ainda sem resposta: anel vazio na trilha pendente
    ".KK.",
    "K..K",
    "K..K",
    ".KK.",
]
HOP_AT = (21, 12)
TUFTS = [(24, 21), (25, 21), (9, 15), (10, 15), (5, 13), (13, 13), (14, 13)]
HILLS = [(4, 12), (5, 12), (6, 12), (7, 12), (9, 12), (10, 12), (11, 12), (25, 12), (26, 12), (27, 12)]
# ajustes finos pixel a pixel, (x, y) -> cor: tiram pixels soltos que
# sobravam entre a serra e o aro, entre o salto e a colina e na sombra
FIXES = {
    (25, 8): "B", (24, 12): "D", (4, 17): "V", (7, 23): "D",
    (16, 26): "C", (18, 26): "C",
}


def scene():
    g = blank(N, N)
    for y in range(N):
        for x in range(N):
            g[y][x] = SKY.get(y, "T") if y < HZ else "V"
    for x, y in RAYS:
        g[y][x] = "S"
    stamp(g, SUN, *SUN_AT)
    for y, (a, b) in SERRA_R.items():
        mid = (a + b) // 2
        for x in range(a, b + 1):
            g[y][x] = "Q" if x <= mid else "B"
    for x in range(N):
        g[HZ][x] = "D"
    for x, y in TUFTS + HILLS:
        g[y][x] = "D"
    # sombra de 1 px por dentro do aro, na metade de baixo
    for y in range(17, N):
        for x in range(N):
            if INSIDE[y][x]:
                if any(not INSIDE[yy][xx] for xx, yy in ((x + 1, y), (x - 1, y), (x, y + 1))):
                    g[y][x] = "D"
    for x, y in ROAD_TODO:
        g[y][x] = "e"
    for x, (y0, y1) in ROAD_DONE.items():
        for y in range(y0, y1 + 1):
            g[y][x] = "C"
        g[y1][x] = "M"
    for x, y in DASHES:
        g[y][x] = "R"
    stamp(g, ARAUCARIA, *ARAU_AT)
    stamp(g, HOP, *HOP_AT)
    # leito de terra em volta da pegada (1 px) e a pegada por cima
    px, py = PAW_AT
    occ = {(px + dx, py + dy) for dy, r in enumerate(PAW) for dx, c in enumerate(r) if c != "."}
    for x, y in occ:
        for xx, yy in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if (xx, yy) not in occ:
                g[yy][xx] = "C"
    # buracos de 1 px cercados de terra viram terra (nada de pixel solto)
    halo = {(xx, yy) for x, y in occ for xx, yy in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1))} - occ
    for y in range(N):
        for x in range(N):
            if (x, y) in occ or (x, y) in halo:
                continue
            nb = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
            if all(p in occ or p in halo for p in nb):
                g[y][x] = "C"
    # leito continuo: em cada linha, o vao entre as bordas da pegada tambem e terra
    bed = occ | halo
    for y in sorted({yy for _, yy in bed}):
        xs = [x for x, yy in bed if yy == y]
        for x in range(min(xs), max(xs) + 1):
            if (x, y) not in occ:
                g[y][x] = "C"
    for x, y in occ:
        g[y][x] = "K"
    for (x, y), c in FIXES.items():
        g[y][x] = c
    return g


def emblem():
    g = scene()
    ring(g)
    return g


TITLE = "Peabiru: pegada de onça na trilha, sob o sol nascente"
DESC = ("Emblema em pixel art: bússola com sol nascente atrás da serra, araucária, trilha de terra "
        "com o tracejado do trace até a pegada de onça, onde o rastro parou, e o resto do caminho "
        "claro, com um salto vazio, até o sol.")


def main():
    g = emblem()
    open(os.path.join(HERE, "g_emblem.txt"), "w").write(dump(g))
    svg = to_svg(g, TITLE, DESC, scale=16)
    open(os.path.join(HERE, "peabiru.svg"), "w", encoding="utf-8").write(svg)
    bad = verify(g, svg)
    print("peabiru.svg", len(svg.encode()), "bytes; verificacao:", "ok" if not bad else bad[:10])


if __name__ == "__main__":
    main()
