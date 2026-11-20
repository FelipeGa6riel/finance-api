use diesel::connection::SimpleConnection;
use diesel::r2d2::{ConnectionManager, CustomizeConnection, Pool};
use diesel::SqliteConnection;
use time::{OffsetDateTime, PrimitiveDateTime};

pub type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;

#[derive(Debug)]
struct SqlitePragmaCustomizer;

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for SqlitePragmaCustomizer {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        conn.batch_execute(
            "PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL;",
        )
        .map_err(diesel::r2d2::Error::QueryError)
    }
}

pub fn build_pool(database_url: &str) -> SqlitePool {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    Pool::builder()
        .max_size(10)
        .connection_customizer(Box::new(SqlitePragmaCustomizer))
        .build(manager)
        .unwrap_or_else(|e| panic!("Error building connection pool: {e}"))
}

pub fn now_utc() -> PrimitiveDateTime {
    let now = OffsetDateTime::now_utc();
    // Truncate nanoseconds so SQLite timestamp format stays second-precision
    let t = time::Time::from_hms(now.hour(), now.minute(), now.second()).expect("valid time");
    PrimitiveDateTime::new(now.date(), t)
}
