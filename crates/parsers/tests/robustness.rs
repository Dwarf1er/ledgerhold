//! Statement files come from outside, so no input may crash a parser. These tests feed every parser
//! hostile and mangled input and require an `Ok`/`Err` answer rather than a panic.

use ledgerhold_parsers::all;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        .collect();
    v.sort();
    v
}

/// Run every parser on `input`; return the name of the first one that panicked.
fn survives(input: &[u8]) -> Result<(), String> {
    for p in all() {
        let id = p.id();
        if catch_unwind(AssertUnwindSafe(|| {
            let _ = p.parse(input);
        }))
        .is_err()
        {
            return Err(id.to_string());
        }
    }
    Ok(())
}

fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut b = base.to_vec();
    for _ in 0..1 + rng.below(8) {
        if b.is_empty() {
            b.push(rng.next() as u8);
            continue;
        }
        let i = rng.below(b.len());
        match rng.below(7) {
            0 => b[i] = rng.next() as u8,
            1 => b.truncate(i),
            2 => {
                b.remove(i);
            }
            3 => {
                let chunk: Vec<u8> = b[i..(i + rng.below(40)).min(b.len())].to_vec();
                b.splice(i..i, chunk);
            }
            4 => {
                let tricky = ["ŉ", "ǰ", "İ", "ß", "\u{feff}", "\0", "\r", "<", "&", "\u{202e}", "9999999999999999999999", "-", "\"", "😀"];
                let t = tricky[rng.below(tricky.len())].as_bytes().to_vec();
                b.splice(i..i, t);
            }
            5 => b[i] ^= 1 << rng.below(8),
            _ => {
                let j = rng.below(b.len());
                b.swap(i, j);
            }
        }
    }
    b
}

#[test]
fn mangled_statements_never_panic() {
    let started = Instant::now();
    for (name, base) in fixtures() {
        // PDFs are far slower to parse; fewer rounds keep the suite quick.
        // LH_FUZZ_SCALE=20 (say) runs a much deeper pass than the default quick one.
        let scale: usize = std::env::var("LH_FUZZ_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        let rounds = scale * if name.ends_with(".pdf") { 60 } else { 1500 };
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ name.len() as u64 ^ (scale as u64) << 32);
        for round in 0..rounds {
            let input = mutate(&mut rng, &base);
            if let Err(parser) = survives(&input) {
                let dump = std::env::temp_dir().join(format!("lh-crash-{name}-{round}"));
                std::fs::write(&dump, &input).unwrap();
                panic!("parser {parser} panicked on a mutation of {name}; input saved to {}", dump.display());
            }
        }
    }
    assert!(started.elapsed() < Duration::from_secs(300), "fuzzing took suspiciously long");
}

#[test]
fn hostile_unicode_around_ofx_markers_does_not_panic() {
    // Characters whose upper-case form has a different byte length shift offsets found in the upper-cased copy.
    for prefix in ["ŉ", "ŉŉŉŉ", "ǰ", "ǰǰǰǰǰǰ", "ß", "İİİ", "ﬃﬃﬃ", "ΐΐΐΐ"] {
        for suffix in ["", "<STMTTRN><TRNAMT>1.00</TRNAMT></STMTTRN>", "<BANKTRANLIST>"] {
            let input = format!("{prefix}<OFX>{suffix}");
            assert!(survives(input.as_bytes()).is_ok(), "{input:?}");
            let input = format!("OFXHEADER:100\nCHARSET:1252\n\n{prefix}<OFX>{suffix}");
            assert!(survives(input.as_bytes()).is_ok(), "{input:?}");
        }
    }
}

#[test]
fn degenerate_inputs_are_answered_not_crashed_on() {
    let big = vec![b'a'; 2_000_000];
    let nested = "<A>".repeat(200_000);
    let long_line = format!("{},{}\n", "x".repeat(1_000_000), "1".repeat(1_000_000));
    for input in [&b""[..], b"\0", b"\xff\xfe\xfd", b"%PDF-", b"%PDF-1.7\n", &big, nested.as_bytes(), long_line.as_bytes()] {
        assert!(survives(input).is_ok());
    }
}
