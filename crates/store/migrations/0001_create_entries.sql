CREATE TABLE IF NOT EXISTS entries (
  id              TEXT PRIMARY KEY,
  domain          TEXT NOT NULL,
  created_at      BIGINT NOT NULL,
  last_seen_at    BIGINT NOT NULL,
  predecessor_id  TEXT NULL REFERENCES entries(id) ON DELETE SET NULL,
  profile_version BIGINT NOT NULL,
  grade           TEXT NOT NULL,
  fingerprint     TEXT NOT NULL,
  run             TEXT NOT NULL,
  diff            TEXT NULL,
  engine_version  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_entries_domain_created_at ON entries (domain, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_entries_predecessor_id ON entries (predecessor_id);
