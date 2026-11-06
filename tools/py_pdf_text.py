"""Dump pdfplumber's extracted text for a PDF (what the legacy parser sees)."""
import sys
import pdfplumber

with pdfplumber.open(sys.argv[1]) as pdf:
    print("\n".join((p.extract_text() or "") for p in pdf.pages))
