use anyhow::Result;
use diesel::{Connection, SqliteConnection};
use std::env;

pub fn establish_connection() -> Result<SqliteConnection> {
    let database_url = env::var("DATABASE_URL")?;
    let conn = SqliteConnection::establish(&database_url)?;
    Ok(conn)
}

pub async fn run_blocking_db<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut SqliteConnection) -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut conn = establish_connection()?;
        f(&mut conn)
    })
    .await?
}
