#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = ocpi::db::get_sqlite_pool().await?;

    let versions: Vec<(String, String)> =
        sqlx::query_as("SELECT version, url FROM versions ORDER BY version")
            .fetch_all(&pool)
            .await?;
    for (version, url) in versions {
        println!("OCPI {version}: {url}");
    }

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM credentials")
        .fetch_one(&pool)
        .await?;
    println!("Example credentials: {count}");

    Ok(())
}
