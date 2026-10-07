# Data Files

ifconfig-rs reads all enrichment data from the files in this directory at startup.
This document explains where each file comes from and how to keep it up to date.

## Quick Start

```sh
# 1. Create your GeoIP credentials file from the template
cp geoipupdate.conf.example .geoip.conf
$EDITOR .geoip.conf   # fill in AccountID and LicenseKey (see below)

# 2. Fetch all data files (from the repository root)
just ifconfig-data
```

`just ifconfig-data` (which runs `fetch.sh`) downloads every data file. It requires
[`geoipupdate`](https://github.com/maxmind/geoipupdate) for the MaxMind databases
and `curl` + `jq` for the rest. Everything else is sourced from public URLs.

---

## MaxMind GeoLite2 (GeoIP City + ASN)

**Files:** `GeoLite2-City.mmdb`, `GeoLite2-ASN.mmdb`
**Fetched by:** `just ifconfig-data`

MaxMind distributes these databases under the
[GeoLite2 End User License Agreement](https://www.maxmind.com/en/geolite2/eula).
A free MaxMind account is required.

### Setup

1. [Create a MaxMind account](https://www.maxmind.com/en/geolite2/signup) and generate
   a license key under *My Account → Services → Manage License Keys*.

2. Install [`geoipupdate`](https://github.com/maxmind/geoipupdate):
   - macOS: `brew install geoipupdate`
   - Debian/Ubuntu: see the [official install guide](https://github.com/maxmind/geoipupdate#installing-on-ubuntu-via-apt)

3. Copy the example config and fill in your credentials:

   ```sh
   cp crates/ifconfig-rs/data/geoipupdate.conf.example crates/ifconfig-rs/data/.geoip.conf
   ```

   Edit `.geoip.conf` — set `AccountID` and `LicenseKey`. The `DatabaseDirectory`
   line is ignored by `fetch.sh` (it passes `-d .` to override it), so only the
   credentials matter.

4. Run `just ifconfig-data` from the repository root (fetches everything).

MaxMind releases updated databases on the first and third Tuesday of each month.

---

## Other Data Sources

All remaining files are fetched from public URLs by `just ifconfig-data` — no account required. A file that already exists is not fetched again; `crates/ifconfig-rs/data/fetch.sh clean` removes them.

| File | Source |
|---|---|
| `regexes.yaml` | [ua-parser/uap-core](https://github.com/ua-parser/uap-core/blob/master/regexes.yaml) |
| `tor_exit_nodes.txt` | [Tor Project bulk exit list](https://check.torproject.org/torbulkexitlist) |
| `feodo_botnet_ips.txt` | [Feodo Tracker](https://feodotracker.abuse.ch/downloads/ipblocklist.txt) |
| `vpn_ranges.txt` | [X4BNet lists\_vpn](https://github.com/X4BNet/lists_vpn) (IPv4 + IPv6) |
| `cloud_provider_ranges.jsonl` | AWS, GCP, Azure, Cloudflare, Oracle, Fastly, DigitalOcean, Linode, GitHub, Google Services (normalized to JSONL) |
| `datacenter_ranges.txt` | [X4BNet datacenter list](https://github.com/X4BNet/lists_vpn) |
| `bot_ranges.jsonl` | Googlebot, Bingbot, Applebot, GPTBot (normalized to JSONL) |
| `spamhaus_drop.txt` | [Spamhaus DROP + EDROP + DROPv6](https://www.spamhaus.org/drop/) (concatenated, comments stripped) |
| `cins_army_ips.txt` | [CINS Army / CI Bad Guys](https://cinsscore.com/list/ci-badguys.txt) (one IPv4 per line, comment lines stripped) |

> **Note on Spamhaus:** The free DROP feeds are rate-limited and intended for
> non-commercial use. A [data feed license](https://www.spamhaus.com/pricing/) is
> required for commercial deployments.

> **Note on CINS Army:** The CI Bad Guys list is published by the Collective
> Intelligence Network Security (CINS) project. It is free for non-commercial use.
> The list is updated frequently; refresh at least daily.

---

## Keeping Data Fresh

### Production (recommended)

Enable filesystem watching in your config and run data updates on a cron:

```toml
# ifconfig.toml
watch_data_files = true   # server reloads automatically when files change
```

```cron
# Weekly refresh — update GeoIP databases and public lists
0 3 * * 2 cd /path/to/netray && just ifconfig-data
```

### Manual reload (without `watch_data_files`)

Send SIGHUP to the running process:

```sh
kill -HUP <pid of `netray ip`>
# or with Docker:
docker kill --signal=HUP <container>
```

---

## Docker Image Shortcut

The `ifconfig-rs-data` Docker image ships all data files pre-built.
Use it to bootstrap a deployment without running `just ifconfig-data`:

```sh
docker create --name ifconfig-data ghcr.io/netray-info/ifconfig-rs-data:latest
docker cp ifconfig-data:/data ./data
docker rm ifconfig-data
```

To build and push an updated data image yourself:

```sh
just ifconfig-data-image             # build locally
just ifconfig-data-image true        # build and push to GHCR
```
