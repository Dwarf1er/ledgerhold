# Ledgerhold

A household bank ledger. Import statements from several banks into one local SQLite database,
skip transactions you already have, track every account's balance from a known starting point,
and export OFX for any date range (for example the 1st to the last day of each month).

It replaces [bank-to-ofx](../bank-to-ofx), keeping its parsers.

## What it does

- **Imports** Desjardins CSV (chequing/savings) and Visa PDF, Wealthsimple CSV (chequing and credit),
  and any OFX/QFX/QBO file. Unreadable rows are reported with line numbers, never silently dropped.
- **Removes duplicates** when statements overlap, even across formats (a CSV followed by the OFX of
  the same account). Two identical same-day purchases stay separate. Every import can be undone as a whole.
- **Tracks balances.** Declare "the balance at the end of 2026-01-01 was $700.00" (an *anchor*), and the
  balance on any other day is calculated from the transactions. Add more anchors from later statements
  and Ledgerhold tells you which period doesn't add up and by how much.
- **Exports OFX** for one month, any range, or every month, one file per account, with the real bank id,
  account type, escaped text and the balance at the end of the range.
- **Shares accounts.** People are linked to accounts many-to-many, so a joint account has two owners.

## Layout

| Path | Purpose |
|---|---|
| `crates/core` | `Money` (integer cents), canonical types, dedup planning. No I/O. |
| `crates/parsers` | One parser per bank/format, tested against goldens from the old Python tool. |
| `crates/ofx` | OFX writer and a tolerant reader (SGML/XML, cp1252, single-line files). |
| `crates/store` | SQLite schema + migrations, import/undo, balances, reconciliation. |
| `crates/app` | Service layer with JSON-friendly DTOs, shared by the CLI and the desktop app. |
| `crates/cli` | The `ledgerhold` command. |
| `src-tauri` | Tauri 2 shell: thin commands over `crates/app`. |
| `ui` | Svelte 5 + Vite + TypeScript front end. |
| `fixtures` | Synthetic statements and goldens (no real data). `tools/` regenerates them. |

## Develop

Tooling is pinned by [mise](https://mise.jdx.dev): `mise install`.

```sh
mise run test      # Rust + UI tests and type checks
mise run ui        # UI in a browser with demo data, no Tauri needed
mise run dev       # desktop app with hot reload
mise run build     # installable bundles
```

The desktop app needs the Tauri system libraries (on Arch: `webkit2gtk-4.1 gtk3 librsvg libayatana-appindicator`).

## Command line

```sh
ledgerhold person add Alex
ledgerhold institution add Desjardins --bank-id 815
ledgerhold account add --institution Desjardins --name Joint --kind checking --number EOP-12345 --owner Alex --owner Sam
ledgerhold anchor set Joint 2026-01-01 700.00
ledgerhold import Joint releve.csv --parser desjardins [--dry-run]
ledgerhold balance Joint --on 2026-01-31
ledgerhold reconcile Joint
ledgerhold export Joint --month 2026-01 -o ~/budget      # or --from/--to, or --all-months
```

The CLI and the desktop app share one database, `$XDG_DATA_HOME/ledgerhold/ledgerhold.sqlite`
(override with `LEDGERHOLD_DB` or `--db`).

## Conventions

- Money is integer minor units; negative means money leaving the account (a card purchase is negative).
- Dates are plain calendar dates.
- An anchor is the balance at the **end** of its day, so transactions on that day are already included.
- Schema changes are new numbered files in `crates/store/migrations`; never edit a shipped one.
