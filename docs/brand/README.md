# Marca do Peabiru

| Arquivo | Uso |
|---|---|
| `peabiru.svg` | emblema quadrado (32x32 px de arte): avatar, favicon, ícone |
| `peabiru-banner.svg` | banner com o nome, no topo do README |

O emblema é o rastro: uma pegada de onça na trilha de terra, inclinada no
sentido do caminho, rumo ao próximo salto (o anel vazio) e ao sol nascente,
com araucária e serra ao fundo. O tracejado vermelho é o trecho já percorrido.
O nome vem do Caminho do Peabiru; o desenho fica na trilha e na paisagem, sem
figura humana nem adornos de nenhum povo.

## Como regenerar

A arte é pixel art de verdade: cada SVG sai de uma grade de caracteres, e cada
caractere é uma cor da paleta em `pix.py`. Só biblioteca padrão do Python.

```sh
python3 docs/brand/emblem.py   # peabiru.svg e g_emblem.txt
python3 docs/brand/banner.py   # peabiru-banner.svg e g_banner.txt
```

`g_emblem.txt` e `g_banner.txt` são as grades geradas, para conferir a arte em
texto. Os scripts verificam que o SVG reproduz a grade pixel a pixel. Os SVGs
não usam `<text>`, imagens, fontes nem scripts, e funcionam nos temas claro e
escuro do GitHub.
