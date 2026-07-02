//! SQLite connection + migrations. The database is the reliability backbone:
//! the queue, node tree, and per-file resume offsets all live here so the
//! engine can be killed and resume cleanly.

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

use crate::Result;

/// Open (creating if needed) the SQLite database file at `path` and run all
/// embedded migrations. Taking a filesystem path (not a URL) avoids URL-parsing
/// pitfalls with absolute Windows paths when the app data dir is passed in.
///
/// WAL + a busy timeout keep the concurrent download workers from tripping over
/// "database is locked" when they persist progress simultaneously.
pub async fn connect(path: impl AsRef<Path>) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    // Migrations live at the workspace root and are embedded at compile time.
    sqlx::migrate!("../../migrations").run(&pool).await?;

    Ok(pool)
}
