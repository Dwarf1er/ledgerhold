//! Debug aid: show the *shape* of each line in the transaction table with its content masked
//! (letters -> A/a, digits -> 9, runs collapsed, "CR" kept), and whether the parser matched it.
//! Contains no merchants, amounts or dates, so the output is safe to share.
//! Usage: cargo run -p ledgerhold-parsers --example table_shape -- statement.pdf
use ledgerhold_parsers::desjardins_credit::{extract_text, is_transaction_line};

fn mask(line: &str) -> String {
    let mut out = String::new();
    let mut last = '\0';
    for tok in line.split(' ') {
        if tok == "CR" {
            out.push_str("CR ");
            last = '\0';
            continue;
        }
        for c in tok.chars() {
            let m = if c.is_ascii_digit() { '9' } else if c.is_uppercase() { 'A' } else if c.is_alphabetic() { 'a' } else { c };
            if m != last || !matches!(m, '9' | 'A' | 'a') {
                out.push(m);
            }
            last = m;
        }
        out.push(' ');
        last = '\0';
    }
    out.trim_end().to_string()
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: table_shape <statement.pdf>");
    let text = extract_text(&std::fs::read(path).unwrap()).unwrap();
    let (mut in_table, mut after, mut before, mut headers) = (false, 0, 0, 0);
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains("Transactions effectuées avec la carte de") {
            in_table = true;
            headers += 1;
            println!("{:>3}: ---- table header ----", n + 1);
            continue;
        }
        let matched = is_transaction_line(line);
        if in_table {
            after += matched as usize;
            println!("{:>3}: {} {}", n + 1, if matched { "MATCH" } else { "  -  " }, mask(line));
        } else if matched || line.starts_with(|c: char| c.is_ascii_digit()) {
            // Rows before the first header are ignored by the parser: show them.
            before += matched as usize;
            println!("{:>3}: BEFORE-HEADER {} {}", n + 1, if matched { "(would match)" } else { "" }, mask(line));
        }
    }
    println!("\nsummary: {headers} header(s), {after} matching row(s) after a header, {before} matching row(s) BEFORE the first header (kept by the parser)");
}
