use crate::{Result, Store, StoreError};
use chrono::NaiveDate;
use ledgerhold_ofx::{OfxAccount, OfxExport, OfxTxn};
use rusqlite::params;

impl Store {
    /// Build OFX export data for `[start, end]` (inclusive) for each account.
    /// FITIDs are the bank's own when known, else a stable derived id, so
    /// exporting the same range twice yields identical ids. `LEDGERBAL` is the
    /// end-of-range balance when the account has an anchor.
    pub fn build_ofx_exports(&self, account_ids: &[String], start: NaiveDate, end: NaiveDate) -> Result<Vec<OfxExport>> {
        if end < start {
            return Err(StoreError::Corrupt(format!("export range ends before it starts: {start}..{end}")));
        }
        let mut out = Vec::new();
        for id in account_ids {
            let account = self.get_account(id)?;
            let bank_id: Option<String> = self.conn.query_row(
                "SELECT ofx_bank_id FROM institution WHERE id = ?1",
                params![account.institution_id],
                |r| r.get(0),
            )?;
            let transactions = self
                .list_transactions(id, Some(start), Some(end))?
                .into_iter()
                .map(|t| OfxTxn { date: t.date, amount: t.amount, fitid: t.export_fitid, description: t.description })
                .collect();
            let ledger_balance = match self.balance_at(id, end) {
                Ok(r) => Some((end, r.balance)),
                Err(StoreError::NoAnchor) => None,
                Err(e) => return Err(e),
            };
            out.push(OfxExport {
                account: OfxAccount {
                    bank_id: bank_id.unwrap_or_else(|| "UNKNOWN".into()),
                    account_id: account.external_account_id.clone().unwrap_or_else(|| account.name.clone()),
                    kind: account.kind,
                    currency: account.currency.clone(),
                },
                start,
                end,
                transactions,
                ledger_balance,
            });
        }
        Ok(out)
    }
}
