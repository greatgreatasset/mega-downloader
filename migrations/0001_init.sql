-- Phase 0 schema. The queue, node tree, and per-file resume state live here so
-- the engine is restart-safe.

CREATE TABLE jobs (
    id          TEXT PRIMARY KEY,                       -- uuid
    link        TEXT NOT NULL,
    kind        TEXT NOT NULL,                          -- 'folder' | 'file'
    root_path   TEXT NOT NULL,                          -- destination dir on disk
    transport   TEXT NOT NULL DEFAULT 'realdebrid',
    status      TEXT NOT NULL DEFAULT 'pending',        -- pending|listing|ready|downloading|done|error
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE nodes (
    id            TEXT PRIMARY KEY,
    job_id        TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    parent_handle TEXT,
    handle        TEXT NOT NULL,                        -- mega node handle
    kind          TEXT NOT NULL,                        -- 'folder' | 'file'
    name          TEXT NOT NULL,
    rel_path      TEXT NOT NULL,                        -- path relative to job root
    size          INTEGER NOT NULL DEFAULT 0,
    file_key      BLOB,                                 -- decrypted node key (files)
    UNIQUE (job_id, handle)
);

CREATE INDEX idx_nodes_job ON nodes (job_id);

CREATE TABLE transfers (
    id          TEXT PRIMARY KEY,
    node_id     TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    status      TEXT NOT NULL DEFAULT 'queued',         -- queued|active|done|error|paused
    bytes_done  INTEGER NOT NULL DEFAULT 0,
    bytes_total INTEGER NOT NULL DEFAULT 0,
    rd_link     TEXT,                                   -- unrestricted real-debrid link
    retries     INTEGER NOT NULL DEFAULT 0,
    error       TEXT,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_transfers_node ON transfers (node_id);

-- Simple key/value store for settings (e.g. RD token, default download dir).
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
