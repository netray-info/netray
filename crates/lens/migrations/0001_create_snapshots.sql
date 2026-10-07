CREATE TABLE IF NOT EXISTS snapshots (
  shortid       TEXT PRIMARY KEY,
  domain        TEXT NOT NULL,
  grade         TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  lens_version  TEXT NOT NULL,
  payload       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_snapshots_created_at ON snapshots (created_at);
