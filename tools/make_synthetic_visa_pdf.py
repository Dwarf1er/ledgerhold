"""Generate a synthetic Desjardins-Visa-like PDF (no real data).

Every table cell is its own positioned text run, like a real statement table,
so text extractors must rebuild lines from coordinates.
Usage: python3 tools/make_synthetic_visa_pdf.py fixtures/synthetic/desjardins_visa.pdf
"""
import sys

def esc(s: str) -> bytes:
    b = s.encode("cp1252")
    return b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")

def page_stream(cells):
    out = [b"BT /F1 10 Tf"]
    last = (0, 0)
    for x, y, text in cells:
        out.append(b"1 0 0 1 %d %d Tm (" % (x, y) + esc(text) + b") Tj")
    out.append(b"ET")
    return b"\n".join(out)

def row(y, txd, txm, pd, pm, desc, amount, credit=False):
    cells = [(50, y, txd), (72, y, txm), (110, y, pd), (132, y, pm), (170, y, desc)]
    cells.append((470, y, amount))
    if credit:
        cells.append((530, y, "CR"))
    return cells

page1 = [
    (50, 780, "Relevé de compte Visa Desjardins"),
    (50, 755, "DATE DU RELEVÉ"), (170, 755, "Jour"), (200, 755, "21"),
    (240, 755, "Mois"), (270, 755, "01"), (310, 755, "Année"), (350, 755, "2026"),
    (50, 735, "Numéro de carte"), (170, 735, "4540 XXXX XXXX 1234"),
    (50, 700, "Transactions effectuées avec la carte de"), (320, 700, "JOHN DOE"),
    (50, 685, "Date"), (110, 685, "Inscr."), (170, 685, "Description"), (470, 685, "Montant"),
]
y = 665
for r in [
    ("30", "12", "31", "12", "RESTAURANT LE GOURMET, QUEBEC QC", "62,30", False),
    ("02", "01", "03", "01", "ESSENCE STATION SERVICE MONTREAL QC", "45,10", False),
    ("05", "01", "06", "01", "PAIEMENT CAISSE", "200,00", True),
    ("10", "01", "12", "01", "REMBOURSEMENT MAGASIN", "15,00", True),
]:
    page1 += row(y, *r)
    y -= 16
page1 += [(50, y - 10, "Sous-total des transactions"), (470, y - 10, "322,40")]

page2 = [(50, 780, "Suite des transactions")]
y = 760
for r in [
    ("15", "01", "16", "01", "ACHAT ORDINATEUR PORTABLE", "1 234,56", False),
    ("20", "01", "21", "01", "INTERETS SUR ACHATS", "8,75", False),
]:
    page2 += row(y, *r)
    y -= 16
page2 += [(50, y - 10, "Total des achats et débits"), (470, y - 10, "1 350,71")]

def build(pages):
    objs = []  # 1-indexed
    def add(b): objs.append(b); return len(objs)
    add(b"<< /Type /Catalog /Pages 2 0 R >>")
    n = len(pages)
    kids = " ".join(f"{4 + 2 * i} 0 R" for i in range(n))
    add(f"<< /Type /Pages /Kids [{kids}] /Count {n} >>".encode())
    add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    for i, cells in enumerate(pages):
        s = page_stream(cells)
        add(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents %d 0 R >>" % (5 + 2 * i))
        add(b"<< /Length %d >>\nstream\n" % len(s) + s + b"\nendstream")
    out = b"%PDF-1.4\n"
    offs = []
    for i, o in enumerate(objs, 1):
        offs.append(len(out))
        out += b"%d 0 obj\n" % i + o + b"\nendobj\n"
    x = len(out)
    out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
    for o in offs:
        out += b"%010d 00000 n \n" % o
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, x)
    return out

open(sys.argv[1], "wb").write(build([page1, page2]))
