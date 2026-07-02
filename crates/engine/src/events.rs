//! Real-time events emitted by the engine and forwarded to the UI over the
//! WebSocket. Broadcast (not per-connection) so progress survives UI reloads.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    JobCreated {
        job_id: String,
        root_name: String,
        total_files: usize,
        total_bytes: i64,
    },
    Progress {
        job_id: String,
        handle: String,
        bytes_done: i64,
        bytes_total: i64,
    },
    /// Real-Debrid couldn't serve this file; falling back to a direct MEGA download.
    FileFallback {
        job_id: String,
        handle: String,
    },
    FileDone {
        job_id: String,
        handle: String,
    },
    FileRetry {
        job_id: String,
        handle: String,
        attempt: u32,
        max: u32,
        reason: String,
    },
    FileError {
        job_id: String,
        handle: String,
        error: String,
    },
    JobDone {
        job_id: String,
    },
}
