use crate::{Result, Store, StoreError, d2s, money, new_id, s2d};
use chrono::NaiveDate;
use ledgerhold_core::{AccountKind, Money};
use rusqlite::{OptionalExtension, params};

/// Avatar colours a person can have. The UI maps each name to a shade.
pub const PERSON_COLORS: [&str; 8] = ["blue", "green", "orange", "red", "purple", "pink", "teal", "slate"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub color: String,
}

fn check_color(color: &str) -> Result<()> {
    if PERSON_COLORS.contains(&color) {
        Ok(())
    } else {
        Err(StoreError::Invalid(format!("unknown colour {color:?} (choose one of: {})", PERSON_COLORS.join(", "))))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Institution {
    pub id: String,
    pub name: String,
    pub ofx_bank_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewAccount {
    pub institution_id: String,
    pub name: String,
    pub kind: AccountKind,
    pub currency: String,
    /// Account/card number as printed by the bank, used to match imports.
    pub external_account_id: Option<String>,
    pub owners: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub institution_id: String,
    pub name: String,
    pub kind: AccountKind,
    pub currency: String,
    pub external_account_id: Option<String>,
    pub archived: bool,
    pub owners: Vec<String>,
}

/// "The balance at the END of `as_of` was `amount`."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub id: String,
    pub account_id: String,
    pub as_of: NaiveDate,
    pub amount: Money,
    pub note: Option<String>,
}

pub(crate) fn kind_from_str(s: &str) -> Result<AccountKind> {
    match s {
        "checking" => Ok(AccountKind::Checking),
        "savings" => Ok(AccountKind::Savings),
        "credit_card" => Ok(AccountKind::CreditCard),
        other => Err(StoreError::Corrupt(format!("unknown account kind {other:?}"))),
    }
}

impl Store {
    /// Add a person. Without a colour, they get the first one nobody else has yet.
    pub fn add_person(&self, name: &str, color: Option<&str>) -> Result<String> {
        let color = match color {
            Some(c) => {
                check_color(c)?;
                c.to_string()
            }
            None => {
                let used: Vec<String> = self.list_people()?.into_iter().map(|p| p.color).collect();
                let free = PERSON_COLORS.iter().find(|c| !used.iter().any(|u| u == *c));
                free.unwrap_or(&PERSON_COLORS[used.len() % PERSON_COLORS.len()]).to_string()
            }
        };
        let id = new_id();
        self.conn.execute("INSERT INTO person(id, name, color) VALUES (?1, ?2, ?3)", params![id, name, color])?;
        Ok(id)
    }

    pub fn list_people(&self) -> Result<Vec<Person>> {
        let mut st = self.conn.prepare("SELECT id, name, COALESCE(color, 'slate') FROM person ORDER BY name, id")?;
        let rows = st.query_map([], |r| Ok(Person { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Change a person's name and, if given, their colour.
    pub fn update_person(&self, id: &str, name: &str, color: Option<&str>) -> Result<()> {
        if let Some(c) = color {
            check_color(c)?;
        }
        let n = self.conn.execute(
            "UPDATE person SET name = ?2, color = COALESCE(?3, color),
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![id, name, color],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("person {id}")));
        }
        Ok(())
    }

    /// Remove a person. Accounts they co-owned stay, just without them as an owner.
    pub fn delete_person(&self, id: &str) -> Result<()> {
        if self.conn.execute("DELETE FROM person WHERE id = ?1", params![id])? == 0 {
            return Err(StoreError::NotFound(format!("person {id}")));
        }
        Ok(())
    }

    pub fn add_institution(&self, name: &str, ofx_bank_id: Option<&str>) -> Result<String> {
        let id = new_id();
        self.conn.execute(
            "INSERT INTO institution(id, name, ofx_bank_id) VALUES (?1, ?2, ?3)",
            params![id, name, ofx_bank_id],
        )?;
        Ok(id)
    }

    pub fn update_institution(&self, id: &str, name: &str, ofx_bank_id: Option<&str>) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE institution SET name = ?2, ofx_bank_id = ?3, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE id = ?1",
            params![id, name, ofx_bank_id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("institution {id}")));
        }
        Ok(())
    }

    /// Remove a bank. Refused while accounts still belong to it, so nothing is lost by accident.
    pub fn delete_institution(&self, id: &str) -> Result<()> {
        let accounts: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM account WHERE institution_id = ?1", params![id], |r| r.get(0))?;
        if accounts > 0 {
            return Err(StoreError::InUse(format!(
                "this bank still has {accounts} account{}; delete {} first",
                if accounts == 1 { "" } else { "s" },
                if accounts == 1 { "it" } else { "them" },
            )));
        }
        if self.conn.execute("DELETE FROM institution WHERE id = ?1", params![id])? == 0 {
            return Err(StoreError::NotFound(format!("institution {id}")));
        }
        Ok(())
    }

    pub fn list_institutions(&self) -> Result<Vec<Institution>> {
        let mut st = self.conn.prepare("SELECT id, name, ofx_bank_id FROM institution ORDER BY name, id")?;
        let rows = st.query_map([], |r| Ok(Institution { id: r.get(0)?, name: r.get(1)?, ofx_bank_id: r.get(2)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_account(&mut self, a: &NewAccount) -> Result<String> {
        let id = new_id();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO account(id, institution_id, name, kind, currency, external_account_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, a.institution_id, a.name, a.kind.as_str(), a.currency, a.external_account_id],
        )?;
        for p in &a.owners {
            tx.execute("INSERT INTO account_owner(account_id, person_id) VALUES (?1, ?2)", params![id, p])?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn rename_account(&self, id: &str, name: &str) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE account SET name = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![id, name],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("account {id}")));
        }
        Ok(())
    }

    /// Change an account's name and printed number (`None` clears the number).
    pub fn update_account(&self, id: &str, name: &str, external_account_id: Option<&str>) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE account SET name = ?2, external_account_id = ?3,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![id, name, external_account_id],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("account {id}")));
        }
        Ok(())
    }

    /// Remove an account with everything under it (transactions, imports, balances, owners).
    /// Returns the number of transactions removed.
    pub fn delete_account(&self, id: &str) -> Result<usize> {
        let n: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM transactions WHERE account_id = ?1", params![id], |r| r.get(0))?;
        if self.conn.execute("DELETE FROM account WHERE id = ?1", params![id])? == 0 {
            return Err(StoreError::NotFound(format!("account {id}")));
        }
        Ok(n as usize)
    }

    /// Replace the owners of an account (a joint account has several).
    pub fn set_owners(&mut self, account_id: &str, owners: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM account_owner WHERE account_id = ?1", params![account_id])?;
        for p in owners {
            tx.execute("INSERT INTO account_owner(account_id, person_id) VALUES (?1, ?2)", params![account_id, p])?;
        }
        tx.execute(
            "UPDATE account SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![account_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn owners_of(&self, account_id: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT person_id FROM account_owner WHERE account_id = ?1 ORDER BY person_id")?;
        let rows = st.query_map(params![account_id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    fn account_from_row(&self, r: &rusqlite::Row<'_>) -> Result<Account> {
        let id: String = r.get(0)?;
        Ok(Account {
            owners: self.owners_of(&id)?,
            id,
            institution_id: r.get(1)?,
            name: r.get(2)?,
            kind: kind_from_str(&r.get::<_, String>(3)?)?,
            currency: r.get(4)?,
            external_account_id: r.get(5)?,
            archived: r.get::<_, i64>(6)? != 0,
        })
    }

    const ACCOUNT_COLS: &'static str = "id, institution_id, name, kind, currency, external_account_id, archived";

    pub fn get_account(&self, id: &str) -> Result<Account> {
        let sql = format!("SELECT {} FROM account WHERE id = ?1", Self::ACCOUNT_COLS);
        let mut st = self.conn.prepare(&sql)?;
        let mut rows = st.query(params![id])?;
        match rows.next()? {
            Some(r) => self.account_from_row(r),
            None => Err(StoreError::NotFound(format!("account {id}"))),
        }
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let sql = format!("SELECT {} FROM account ORDER BY name, id", Self::ACCOUNT_COLS);
        let mut st = self.conn.prepare(&sql)?;
        let mut rows = st.query([])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(self.account_from_row(r)?);
        }
        Ok(out)
    }

    pub fn find_account_by_external(&self, institution_id: &str, external: &str) -> Result<Option<Account>> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM account WHERE institution_id = ?1 AND external_account_id = ?2",
                params![institution_id, external],
                |r| r.get(0),
            )
            .optional()?;
        id.map(|id| self.get_account(&id)).transpose()
    }

    /// Set (or replace) the starting balance for the end of `as_of`.
    pub fn set_anchor(&self, account_id: &str, as_of: NaiveDate, amount: Money, note: Option<&str>) -> Result<String> {
        self.get_account(account_id)?;
        let id = new_id();
        self.conn.execute(
            "INSERT INTO balance_anchor(id, account_id, as_of_date, amount, note) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account_id, as_of_date) DO UPDATE SET amount = excluded.amount, note = excluded.note",
            params![id, account_id, d2s(as_of), amount.minor(), note],
        )?;
        Ok(self.conn.query_row(
            "SELECT id FROM balance_anchor WHERE account_id = ?1 AND as_of_date = ?2",
            params![account_id, d2s(as_of)],
            |r| r.get(0),
        )?)
    }

    pub fn list_anchors(&self, account_id: &str) -> Result<Vec<Anchor>> {
        let mut st = self.conn.prepare(
            "SELECT id, account_id, as_of_date, amount, note FROM balance_anchor WHERE account_id = ?1 ORDER BY as_of_date",
        )?;
        let rows = st.query_map(params![account_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get(4)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, account_id, date, amount, note) = row?;
            out.push(Anchor { id, account_id, as_of: s2d(&date)?, amount: money(amount), note });
        }
        Ok(out)
    }

    pub fn delete_anchor(&self, id: &str) -> Result<()> {
        let n = self.conn.execute("DELETE FROM balance_anchor WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("anchor {id}")));
        }
        Ok(())
    }
}
