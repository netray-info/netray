SSLMATE_URL := https://web.api.sslmate.com/caahelper/issuers
CCADB_URL   := https://ccadb.my.salesforce-sites.com/ccadb/AllCAAIdentifiersReportCSVV2

.PHONY: all fetch process clean

all: caa_domains.tsv ## Fetch raw data and generate caa_domains.tsv

fetch: sslmate_issuers.json ccadb_caa_identifiers.csv ## Fetch raw data files

sslmate_issuers.json:
	curl -fsSL $(SSLMATE_URL) -o $@

ccadb_caa_identifiers.csv:
	curl -fsSL $(CCADB_URL) -o $@

caa_domains.tsv: sslmate_issuers.json ccadb_caa_identifiers.csv process.py
	python3 process.py

clean: ## Remove fetched raw data files (caa_domains.tsv is kept — it is committed)
	rm -f sslmate_issuers.json ccadb_caa_identifiers.csv
