//! Tabela de texto alinhada à esquerda (`peabiru graph`).

/// Renderiza `headers` + `rows` com colunas separadas por dois espaços,
/// cada coluna com a largura do maior valor. Sem linhas → só o cabeçalho.
pub fn render(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().take(cols).enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let mut out = String::new();
    let header: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    push_row(&mut out, &header, &widths);
    for row in rows {
        push_row(&mut out, row, &widths);
    }
    out
}

fn push_row(out: &mut String, row: &[String], widths: &[usize]) {
    let mut line = String::new();
    for (i, width) in widths.iter().enumerate() {
        let cell = row.get(i).map_or("", String::as_str);
        if i > 0 {
            line.push_str("  ");
        }
        line.push_str(cell);
        if i + 1 < widths.len() {
            let pad = width.saturating_sub(cell.chars().count());
            line.extend(std::iter::repeat_n(' ', pad));
        }
    }
    out.push_str(line.trim_end());
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_columns_and_trims_last() {
        let out = render(
            &["NAME", "KIND"],
            &[
                vec!["a".into(), "pod".into()],
                vec!["longer-name".into(), "x".into()],
            ],
        );
        assert_eq!(out, "NAME         KIND\na            pod\nlonger-name  x\n");
    }

    #[test]
    fn missing_cells_are_blank() {
        let out = render(&["A", "B", "C"], &[vec!["1".into()]]);
        assert_eq!(out, "A  B  C\n1\n");
    }
}
