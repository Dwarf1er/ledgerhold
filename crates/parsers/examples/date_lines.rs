//! Debug aid: print only the extracted lines that may hold the statement date (no transactions).
//! Usage: cargo run -p ledgerhold-parsers --example date_lines -- statement.pdf
fn main() {
    let path = std::env::args().nth(1).expect("usage: date_lines <statement.pdf>");
    let text = ledgerhold_parsers::desjardins_credit::extract_text(&std::fs::read(path).unwrap()).unwrap();
    let keys = ["relev", "jour", "mois", "ann", "date", "période", "periode"];
    for (n, line) in text.lines().enumerate().take(80) {
        let l = line.to_lowercase();
        if keys.iter().any(|k| l.contains(k)) {
            println!("{:>3}: {:?}", n + 1, line);
        }
    }
}
