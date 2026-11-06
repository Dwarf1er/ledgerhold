use crate::ParseError;
use chrono::NaiveDate;

/// Decode bytes to text: UTF-8 (BOM stripped) or UTF-16 with BOM.
pub fn decode_text(bytes: &[u8]) -> Result<String, ParseError> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(rest.to_vec()).map_err(|e| ParseError::Encoding(e.to_string()));
    }
    for (bom, le) in [([0xFF, 0xFE], true), ([0xFE, 0xFF], false)] {
        if let Some(rest) = bytes.strip_prefix(&bom) {
            if rest.len() % 2 != 0 {
                return Err(ParseError::Encoding("odd-length UTF-16".into()));
            }
            let units: Vec<u16> = rest
                .chunks_exact(2)
                .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
                .collect();
            return String::from_utf16(&units).map_err(|e| ParseError::Encoding(e.to_string()));
        }
    }
    String::from_utf8(bytes.to_vec()).map_err(|e| ParseError::Encoding(e.to_string()))
}

pub fn parse_iso_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| format!("invalid date {s:?} (expected YYYY-MM-DD)"))
}

/// Column lookup by (trimmed, case-insensitive) header name.
pub struct Headers(Vec<String>);

impl Headers {
    pub fn new(rec: &csv::StringRecord) -> Self {
        Headers(rec.iter().map(|h| h.trim().to_lowercase()).collect())
    }
    pub fn find(&self, name: &str) -> Option<usize> {
        self.0.iter().position(|h| h == name)
    }
    pub fn require(&self, name: &'static str) -> Result<usize, ParseError> {
        self.find(name).ok_or(ParseError::MissingColumn(name))
    }
}

pub fn line_of(rec: &csv::StringRecord) -> String {
    match rec.position() {
        Some(p) => format!("line {}", p.line()),
        None => "unknown line".into(),
    }
}
