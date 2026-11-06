//! Money as integer minor units (cents). Never floats.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoneyError {
    #[error("empty amount")]
    Empty,
    #[error("invalid amount: {0:?}")]
    Invalid(String),
    #[error("amount out of range: {0:?}")]
    Overflow(String),
}

/// Signed amount in minor units (cents). Negative = money out of the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(pub i64);

impl Money {
    pub const ZERO: Money = Money(0);

    pub fn minor(self) -> i64 {
        self.0
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn abs(self) -> Money {
        Money(self.0.abs())
    }

    /// Parse a decimal string into cents (2 decimal places).
    ///
    /// Accepts: `-12.34`, `+12.34`, `12,34` (comma decimal), `1 234,56`
    /// (French, incl. NBSP / narrow NBSP), `1,234.56`, `$12.34`, `(12.34)`
    /// (negative), and trailing `CR`/`DR` markers (`CR` = positive,
    /// `DR` = negative, applied only if no explicit sign is present).
    pub fn parse(input: &str) -> Result<Money, MoneyError> {
        let original = input;
        let mut s = input.trim().to_string();
        if s.is_empty() {
            return Err(MoneyError::Empty);
        }

        let mut negative = false;
        let mut explicit_sign = false;

        if s.starts_with('(') && s.ends_with(')') {
            negative = true;
            explicit_sign = true;
            s = s[1..s.len() - 1].trim().to_string();
        }

        let upper = s.to_uppercase();
        let mut suffix: Option<bool> = None; // Some(true)=negative
        if upper.ends_with("CR") {
            suffix = Some(false);
            s.truncate(s.len() - 2);
        } else if upper.ends_with("DR") {
            suffix = Some(true);
            s.truncate(s.len() - 2);
        }
        let mut s = s.trim().to_string();

        if let Some(rest) = s.strip_prefix('-').or_else(|| s.strip_prefix('\u{2212}')) {
            negative = true;
            explicit_sign = true;
            s = rest.trim().to_string();
        } else if let Some(rest) = s.strip_prefix('+') {
            explicit_sign = true;
            s = rest.trim().to_string();
        }
        if s.ends_with('-') {
            negative = true;
            explicit_sign = true;
            s.pop();
        }
        if !explicit_sign && let Some(neg) = suffix {
            negative = neg;
        }

        // Strip currency symbols and all kinds of space.
        let cleaned: String = s
            .chars()
            .filter(|c| !c.is_whitespace() && !matches!(c, '$' | '€' | '£' | '\u{a0}' | '\u{202f}'))
            .collect();
        if cleaned.is_empty() || !cleaned.chars().any(|c| c.is_ascii_digit()) {
            return Err(MoneyError::Invalid(original.to_string()));
        }
        if !cleaned.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') {
            return Err(MoneyError::Invalid(original.to_string()));
        }

        // The last separator is the decimal mark if followed by 1-2 digits
        // (or if it is the only kind of separator and appears once with !=3 digits).
        let last_sep = cleaned.rfind(['.', ',']);
        let (int_part, frac_part) = match last_sep {
            None => (cleaned.clone(), String::new()),
            Some(i) => {
                let after = &cleaned[i + 1..];
                let sep = cleaned.as_bytes()[i] as char;
                let same_count = cleaned.matches(sep).count();
                // Money has at most 2 decimals, so a separator followed by exactly
                // 3 digits is a thousands separator ("1,234" / "1.234").
                let is_decimal = same_count == 1 && after.len() <= 2;
                if is_decimal {
                    (cleaned[..i].to_string(), after.to_string())
                } else {
                    (cleaned.clone(), String::new())
                }
            }
        };
        let int_digits: String = int_part.chars().filter(|c| c.is_ascii_digit()).collect();
        if int_digits.is_empty() && frac_part.is_empty() {
            return Err(MoneyError::Invalid(original.to_string()));
        }
        let int_val: i64 = if int_digits.is_empty() {
            0
        } else {
            int_digits.parse().map_err(|_| MoneyError::Overflow(original.to_string()))?
        };
        let mut frac = frac_part;
        while frac.len() < 2 {
            frac.push('0');
        }
        let frac_val: i64 = frac.parse().map_err(|_| MoneyError::Invalid(original.to_string()))?;
        let cents = int_val
            .checked_mul(100)
            .and_then(|v| v.checked_add(frac_val))
            .ok_or_else(|| MoneyError::Overflow(original.to_string()))?;
        Ok(Money(if negative { -cents } else { cents }))
    }
}

impl std::ops::Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}
impl std::ops::Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}
impl std::ops::Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}
impl std::iter::Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        Money(iter.map(|m| m.0).sum())
    }
}

impl fmt::Display for Money {
    /// Plain decimal, always two places, `.` separator (OFX `TRNAMT` style).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> i64 {
        Money::parse(s).unwrap().0
    }

    #[test]
    fn plain_and_signed() {
        assert_eq!(p("12.34"), 1234);
        assert_eq!(p("-12.34"), -1234);
        assert_eq!(p("+0.5"), 50);
        assert_eq!(p("7"), 700);
        assert_eq!(p("-.99"), -99);
    }

    #[test]
    fn french_format() {
        assert_eq!(p("1 234,56"), 123456);
        assert_eq!(p("1\u{a0}234,56"), 123456);
        assert_eq!(p("12,34"), 1234);
        assert_eq!(p("-1 234,56"), -123456);
    }

    #[test]
    fn english_thousands() {
        assert_eq!(p("1,234.56"), 123456);
        assert_eq!(p("1,234,567.89"), 123456789);
        assert_eq!(p("$1,000"), 100000);
    }

    #[test]
    fn parens_and_suffix() {
        assert_eq!(p("(12.34)"), -1234);
        assert_eq!(p("45,67 CR"), 4567);
        assert_eq!(p("45,67 DR"), -4567);
        assert_eq!(p("-45,67CR"), -4567);
        assert_eq!(p("12.34-"), -1234);
    }

    #[test]
    fn errors() {
        assert_eq!(Money::parse(""), Err(MoneyError::Empty));
        assert!(Money::parse("abc").is_err());
        assert!(Money::parse("$").is_err());
        assert!(Money::parse("1.2.3x").is_err());
    }

    #[test]
    fn display_roundtrip() {
        assert_eq!(Money(-1234).to_string(), "-12.34");
        assert_eq!(Money(5).to_string(), "0.05");
        assert_eq!(Money(100000).to_string(), "1000.00");
    }
}
