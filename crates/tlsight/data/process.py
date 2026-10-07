#!/usr/bin/env python3
"""Merge SSLMate CAA issuer list and CCADB CAA identifiers into caa_domains.tsv.

Sources:
  SSLMate: https://web.api.sslmate.com/caahelper/issuers
  CCADB:   https://ccadb.my.salesforce-sites.com/ccadb/AllCAAIdentifiersReportCSVV2

Output: caa_domains.tsv — two tab-separated columns: caa_domain <TAB> ca_name
Sorted by caa_domain. Committed to the repo; read by build.rs at compile time.

Run via: make -C data  (or: make data from project root)
"""

import csv
import json
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent


def load_sslmate(path: Path) -> dict[str, str]:
    """Returns {caa_domain: ca_name} using each CA's own (non-delegated) domains."""
    with open(path) as f:
        issuers = json.load(f)

    result: dict[str, str] = {}
    for ca in issuers:
        name = ca.get("name", "").strip()
        if not name:
            continue
        domains = set(ca.get("domains", []))
        delegated = set(ca.get("delegated_domains", []))
        for domain in domains - delegated:
            domain = domain.strip().lower()
            if domain and domain != "none":
                result[domain] = name
    return result


def load_ccadb(path: Path) -> dict[str, str]:
    """Returns {caa_domain: ca_name} from CCADB CAA identifiers CSV.

    Columns (V2): Subject, Subject Key Identifier (Hex),
                  Intermediate Certificate, Recognized CAA Domains
    """
    result: dict[str, str] = {}
    with open(path, newline="", encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        for row in reader:
            domains_raw = (
                row.get("Recognized CAA Domains")
                or row.get("RecognizedCAADomains")
                or ""
            ).strip()
            subject = (row.get("Subject") or "").strip()
            if not domains_raw or not subject:
                continue

            ca_name = _extract_o_field(subject) or subject

            # Domains may be separated by semicolons, commas, or newlines.
            for sep in (";", ",", "\n"):
                if sep in domains_raw:
                    for domain in domains_raw.split(sep):
                        domain = domain.strip().lower()
                        if domain:
                            result[domain] = ca_name
                    break
            else:
                domain = domains_raw.strip().lower()
                if domain:
                    result[domain] = ca_name

    return result


def _extract_o_field(dn: str) -> str | None:
    """Extract the O= value from an RFC 4514 DN string."""
    for part in dn.split(","):
        part = part.strip()
        if part.startswith("O="):
            return part[2:].strip()
    return None


def merge(sslmate: dict[str, str], ccadb: dict[str, str]) -> dict[str, str]:
    """Merge two {domain: name} dicts. SSLMate wins on conflict (more curated)."""
    merged = {}
    merged.update(ccadb)
    merged.update(sslmate)
    return merged


def main() -> None:
    sslmate_path = SCRIPT_DIR / "sslmate_issuers.json"
    ccadb_path = SCRIPT_DIR / "ccadb_caa_identifiers.csv"
    out_path = SCRIPT_DIR / "caa_domains.tsv"

    if not sslmate_path.exists():
        print(f"error: {sslmate_path} not found; run 'make fetch'", file=sys.stderr)
        sys.exit(1)

    sslmate = load_sslmate(sslmate_path)
    print(f"SSLMate: {len(sslmate)} entries", file=sys.stderr)

    if ccadb_path.exists():
        ccadb = load_ccadb(ccadb_path)
        print(f"CCADB:   {len(ccadb)} entries", file=sys.stderr)
    else:
        ccadb = {}
        print("CCADB:   skipped (file not found)", file=sys.stderr)

    merged = merge(sslmate, ccadb)
    print(f"Merged:  {len(merged)} unique entries", file=sys.stderr)

    with open(out_path, "w") as f:
        for domain in sorted(merged):
            name = merged[domain].replace("\t", " ")
            f.write(f"{domain}\t{name}\n")

    print(f"Written: {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
