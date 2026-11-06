"""Run the legacy Python bank-to-ofx parsers on a fixture and dump JSON.

Usage: bank-to-ofx/.venv/bin/python tools/py_golden.py <parser-id> <file> > golden.json
Output: {account_id: [{date, amount_cents, memo, balance_cents}]}
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path.home() / "src" / "bank-to-ofx"))
from bank_to_ofx.parsers import PARSERS  # noqa: E402


def main() -> None:
    parser_id, path = sys.argv[1], sys.argv[2]
    result = PARSERS[parser_id]().parse(path)
    out = {}
    for account, txns in result.items():
        out[account] = [
            {
                "date": t["date"].strftime("%Y-%m-%d"),
                "amount_cents": round(t["amount"] * 100),
                "memo": t["memo"],
                "balance_cents": None if t.get("balance") is None else round(t["balance"] * 100),
            }
            for t in txns
        ]
    json.dump(out, sys.stdout, indent=2, ensure_ascii=False)
    print()


if __name__ == "__main__":
    main()
