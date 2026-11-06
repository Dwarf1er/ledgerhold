//! Tolerant OFX reader: SGML (1.x, leaf tags unclosed) and XML (2.x), including
//! files emitted as one unbroken line. Produces one [`ParsedStatement`] per
//! `STMTRS`/`CCSTMTRS`, so multi-account files work.

use chrono::NaiveDate;
use ledgerhold_core::{AccountKind, BalanceCheckpoint, Money, ParsedStatement, ParsedTransaction, RowError};

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("no OFX data found (no <OFX> element)")]
    NotOfx,
}

fn decode(bytes: &[u8]) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(600)]).to_uppercase();
    let wants_1252 = head.contains("CHARSET:1252") || head.contains("CHARSET=\"WINDOWS-1252\"");
    if !wants_1252 {
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.strip_prefix('\u{feff}').unwrap_or(s).to_string();
        }
    }
    encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn parse_date(v: &str) -> Option<NaiveDate> {
    let digits: String = v.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    NaiveDate::parse_from_str(digits.get(..8)?, "%Y%m%d").ok()
}

#[derive(Default)]
struct Txn {
    posted: Option<String>,
    user: Option<String>,
    amount: Option<String>,
    fitid: Option<String>,
    name: Option<String>,
    memo: Option<String>,
}

#[derive(Default)]
struct Stmt {
    st: Option<ParsedStatement>,
    n_txn: usize,
    bal_amt: Option<String>,
    bal_date: Option<String>,
    acct_type: Option<String>,
}

pub fn read(bytes: &[u8]) -> Result<Vec<ParsedStatement>, ReadError> {
    let text = decode(bytes);
    let upper_start = text.to_uppercase().find("<OFX>").ok_or(ReadError::NotOfx)?;
    let body = &text[upper_start..];

    let mut out: Vec<ParsedStatement> = Vec::new();
    let mut cur = Stmt::default();
    let mut txn: Option<Txn> = None;
    let mut ctx = String::new(); // innermost interesting aggregate: LEDGERBAL, AVAILBAL, ...

    let flush_txn = |cur: &mut Stmt, txn: &mut Option<Txn>| {
        let Some(t) = txn.take() else { return };
        let Some(st) = cur.st.as_mut() else { return };
        cur.n_txn += 1;
        let r = (|| -> Result<ParsedTransaction, String> {
            let posted = t.posted.as_deref().and_then(parse_date).ok_or("missing or invalid DTPOSTED")?;
            let date = t.user.as_deref().and_then(parse_date).unwrap_or(posted);
            let amount = Money::parse(t.amount.as_deref().ok_or("missing TRNAMT")?).map_err(|e| format!("TRNAMT: {e}"))?;
            let name = t.name.unwrap_or_default();
            let memo = t.memo.unwrap_or_default();
            let description = match (name.is_empty(), memo.is_empty()) {
                (_, true) => name,
                (true, false) => memo,
                (false, false) if memo.starts_with(&name) => memo,
                (false, false) => format!("{name} {memo}"),
            };
            Ok(ParsedTransaction {
                date,
                posted_date: Some(posted),
                amount,
                description,
                fitid: t.fitid.filter(|f| !f.is_empty()),
                source_seq: None,
                balance: None,
            })
        })();
        match r {
            Ok(tx) => st.transactions.push(tx),
            Err(message) => st.errors.push(RowError { row_ref: format!("transaction #{}", cur.n_txn), message }),
        }
    };

    let finish = |cur: &mut Stmt, out: &mut Vec<ParsedStatement>| {
        if let Some(mut st) = cur.st.take() {
            if let (Some(a), Some(d)) = (cur.bal_amt.as_deref(), cur.bal_date.as_deref().and_then(parse_date)) {
                match Money::parse(a) {
                    Ok(balance) => st.checkpoints.push(BalanceCheckpoint { as_of: d, balance }),
                    Err(e) => st.errors.push(RowError { row_ref: "LEDGERBAL".into(), message: e.to_string() }),
                }
            }
            if let Some(ty) = cur.acct_type.as_deref()
                && st.kind != AccountKind::CreditCard
                && ty.eq_ignore_ascii_case("SAVINGS")
            {
                st.kind = AccountKind::Savings;
            }
            out.push(st);
        }
        *cur = Stmt::default();
    };

    let mut rest = body;
    let mut period: (Option<NaiveDate>, Option<NaiveDate>) = (None, None);
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        let Some(gt) = rest.find('>') else { break };
        let tag_raw = &rest[..gt];
        rest = &rest[gt + 1..];
        let value_end = rest.find('<').unwrap_or(rest.len());
        let value = unescape(rest[..value_end].trim());

        if tag_raw.starts_with('?') || tag_raw.starts_with('!') {
            continue;
        }
        let closing = tag_raw.starts_with('/');
        let tag = tag_raw.trim_start_matches('/').split_whitespace().next().unwrap_or("").to_uppercase();

        match (closing, tag.as_str()) {
            (false, "STMTRS" | "CCSTMTRS") => {
                finish(&mut cur, &mut out);
                period = (None, None);
                let kind = if tag == "CCSTMTRS" { AccountKind::CreditCard } else { AccountKind::Checking };
                cur.st = Some(ParsedStatement::new(kind, "CAD"));
            }
            (true, "STMTRS" | "CCSTMTRS") => {
                flush_txn(&mut cur, &mut txn);
                if let (Some(st), (Some(a), Some(b))) = (cur.st.as_mut(), period) {
                    st.period = Some((a, b));
                }
                finish(&mut cur, &mut out);
            }
            (false, "STMTTRN") => {
                flush_txn(&mut cur, &mut txn);
                txn = Some(Txn::default());
            }
            (true, "STMTTRN" | "BANKTRANLIST") => flush_txn(&mut cur, &mut txn),
            (false, "LEDGERBAL" | "AVAILBAL" | "BANKACCTFROM" | "CCACCTFROM") => ctx = tag.clone(),
            (true, "LEDGERBAL" | "AVAILBAL" | "BANKACCTFROM" | "CCACCTFROM") => ctx.clear(),
            (false, leaf) if !value.is_empty() => {
                if let Some(t) = txn.as_mut() {
                    let v = Some(value.clone());
                    match leaf {
                        "DTPOSTED" => t.posted = v,
                        "DTUSER" => t.user = v,
                        "TRNAMT" => t.amount = v,
                        "FITID" => t.fitid = v,
                        "NAME" => t.name = v,
                        "MEMO" => t.memo = v,
                        _ => {}
                    }
                } else if cur.st.is_some() {
                    match (ctx.as_str(), leaf) {
                        ("LEDGERBAL", "BALAMT") => cur.bal_amt = Some(value),
                        ("LEDGERBAL", "DTASOF") => cur.bal_date = Some(value),
                        ("BANKACCTFROM" | "CCACCTFROM", "ACCTID") => {
                            if let Some(st) = cur.st.as_mut() {
                                st.account_hint = Some(value);
                            }
                        }
                        ("BANKACCTFROM", "ACCTTYPE") => cur.acct_type = Some(value),
                        ("", "CURDEF") => {
                            if let Some(st) = cur.st.as_mut() {
                                st.currency = value.to_uppercase();
                            }
                        }
                        ("", "DTSTART") => period.0 = parse_date(&value),
                        ("", "DTEND") => period.1 = parse_date(&value),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    flush_txn(&mut cur, &mut txn);
    finish(&mut cur, &mut out);
    Ok(out)
}
