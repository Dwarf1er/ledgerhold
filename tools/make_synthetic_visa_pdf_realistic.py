"""Generate a synthetic Desjardins-Visa PDF that reproduces the quirks of real statements.

No real data: merchants, amounts and the card number are invented. Quirks reproduced (each one
broke the Rust importer once):
  * content-stream order != reading order: 11 table rows come BEFORE the
    "Transactions effectuées avec la carte de" header line, 11 after it
  * statement date printed as plain "DATE DU RELEVÉ 08 06 2026"; the Jour/Mois/Année labels are
    separate header cells elsewhere
  * a "Remises" (rewards) column ("1,00 %") between the description and the amount
  * a payment with the "CR" glued to the amount ("450,00CR")
  * a font whose Widths array is longer than LastChar - FirstChar + 1 (pdf-extract panics)
  * masked card number with asterisks ("4510 12** **** 7777"), not X's
  * noise lines that must not parse as transactions (rate tables, subtotals, due date)
  * a second, empty table header at the end
Usage: python3 tools/make_synthetic_visa_pdf_realistic.py fixtures/synthetic/desjardins_visa_realistic.pdf
"""
import sys


def esc(s):
    b = s.encode("cp1252")
    return b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")


def stream(cells):
    out = [b"BT /F1 9 Tf"]
    for x, y, text in cells:
        out.append(b"1 0 0 1 %d %d Tm (" % (x, y) + esc(text) + b") Tj")
    out.append(b"ET")
    return b"\n".join(out)


def rows(y, data):
    cells = []
    for d, m, pd, pm, desc, rewards, amount, credit in data:
        cells += [(40, y, d), (60, y, m), (95, y, pd), (115, y, pm), (150, y, desc)]
        if rewards:
            cells.append((400, y, rewards))
        cells.append((470, y, amount))
        if credit:
            cells.append((540, y, "CR"))
        y -= 14
    return cells, y


# (day, month, post day, post month, description, rewards, amount, separate-CR-cell)
BLOCK_A = [
    ("12", "05", "13", "05", "SUPERMARCHE DU COIN QUEBEC QC", "1,00 %", "84,12", False),
    ("13", "05", "14", "05", "PHARMACIE CENTRALE QUEBEC QC", "1,00 %", "23,45", False),
    ("14", "05", "15", "05", "RESTO-BAR LE PORT (QC) QUEBEC QC", "0,50 %", "61,80", False),
    ("15", "05", "16", "05", "STATION ESSO #4821 LEVIS QC", "0,50 %", "52,30", False),
    ("16", "05", "17", "05", "PAYPAL *JEUXVIDEO 4029357733 ON", "0,50 %", "14,99", False),
    ("18", "05", "19", "05", "STATIONNEMENT-CENTRE-VILLE QUEBEC QC", "0,50 %", "12,00", False),
    ("20", "05", "21", "05", "LIBRAIRIE A.B.C. QUEBEC QC", "1,00 %", "37,25", False),
    ("22", "05", "23", "05", "AMZN MKTP CA*AB12C3DE4 WWW.AMAZON.CA ON", "0,50 %", "129,99", False),
    ("25", "05", "26", "05", "BOULANGERIE-PATISSERIE MARIE QC", "1,00 %", "18,60", False),
    ("28", "05", "29", "05", "PAIEMENT - MERCI", "", "450,00CR", False),  # CR glued to the amount
    ("30", "05", "31", "05", "STATION-SERVICE #12 LEVIS QC", "0,50 %", "48,15", False),
]
BLOCK_B = [
    ("01", "06", "02", "06", "CAFE DU MARCHE QUEBEC QC", "1,00 %", "9,75", False),
    ("02", "06", "03", "06", "CINEMA #7 QUEBEC QC", "0,50 %", "32,00", False),
    ("03", "06", "04", "06", "QUINCAILLERIE-BRICO LEVIS QC", "1,00 %", "76,40", False),
    ("03", "06", "04", "06", "REMBOURSEMENT QUINCAILLERIE", "", "10,00", True),
    ("04", "06", "05", "06", "VOYAGES ETE INC QUEBEC QC", "0,50 %", "1 204,50", False),
    ("05", "06", "06", "06", "MARCHE-AUX-FRUITS QUEBEC QC", "1,00 %", "27,35", False),
    ("05", "06", "06", "06", "STATION ESSO #4821 LEVIS QC", "0,50 %", "55,10", False),
    ("06", "06", "07", "06", "FRAIS D'INTERET ACHATS", "", "21,07", False),
    ("06", "06", "07", "06", "TIM HORTONS #0412 QUEBEC QC", "1,00 %", "6,45", False),
    ("07", "06", "08", "06", "RESTAURANT-LE-GRILL QUEBEC QC", "0,50 %", "88,90", False),
    ("07", "06", "08", "06", "PAIEMENT - MERCI", "", "100,00", True),
]

page1 = []
# 1. Table rows first (stream order), like the real extractor output.
a, y = rows(500, BLOCK_A)
page1 += a
page1 += [(40, y - 4, "1 245,00 + 0,00 + 21,07 = 1 266,07"), (40, y - 18, "1,00% 0,50%")]
# 2. Header block: statement date as plain DD MM YYYY, labels in separate cells.
page1 += [
    (40, 760, "Relevé de compte Visa Desjardins"),
    (40, 745, "Date du"), (120, 745, "Jour"), (160, 745, "Mois"), (200, 745, "Année"),
    (40, 733, "relevé"),
    (40, 700, "DATE DU RELEVÉ 08 06 2026"),
    (300, 700, "Date d'échéance : 29 06 2026"),
    (40, 685, "Numéro de carte 4510 12** **** 7777"),
]
# 3. The table header line, after the first block of rows.
page1 += [(40, 380, "Transactions effectuées avec la carte de"), (320, 380, "JANE DOE")]
page1 += [
    (40, 366, "Date de transaction"), (110, 366, "Date  d'inscription"),
    (200, 366, "Description"), (390, 366, "Remises"), (470, 366, "Montant"),
]
# 4. Noise that must not parse (rate table, minimum payment text).
page1 += [
    (40, 352, "Taux d'intérêt annuels"),
    (40, 340, "Achats  20,99 %  20,99"),
    (40, 328, "Avances de fonds  22,49 %  22,49"),
    (40, 316, "Si vous payez seulement votre paiement minimum dû à chaque relevé, vous paierez 5 ans"),
]
# 5. Second block of rows after the header.
b, y = rows(290, BLOCK_B)
page1 += b
page1 += [(40, y - 4, "Sous-total des transactions  1 266,07"), (40, y - 18, "Total pour la carte 4510 12** **** 7777")]
# 6. A second, empty header (e.g. an additional cardholder with no purchases).
page1 += [(40, 60, "Transactions effectuées avec la carte de"), (320, 60, "JOHN DOE")]
page1 += [(40, 46, "Aucune transaction pour cette période")]


def build(pages):
    objs = []

    def add(b):
        objs.append(b)
        return len(objs)

    add(b"<< /Type /Catalog /Pages 2 0 R >>")
    kids = " ".join(f"{4 + 2 * i} 0 R" for i in range(len(pages)))
    add(f"<< /Type /Pages /Kids [{kids}] /Count {len(pages)} >>".encode())
    # Widths has 268 entries but FirstChar..LastChar spans 256: pdf-extract asserts on that.
    widths = b" ".join([b"500"] * 268)
    add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding"
        b" /FirstChar 0 /LastChar 255 /Widths [" + widths + b"] >>")
    for i, cells in enumerate(pages):
        s = stream(cells)
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


open(sys.argv[1], "wb").write(build([page1]))
