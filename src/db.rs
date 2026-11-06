use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migration");

pub async fn get_sqlite_pool() -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename("ocpi.db")
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn initializes_and_reads_example_ocpi_data() -> anyhow::Result<()> {
        let pool = get_sqlite_pool().await?;
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT version, url FROM versions ORDER BY version")
                .fetch_all(&pool)
                .await?;
        assert_eq!(rows.len(), 2);
        for (version, url) in rows {
            let _: ocpi_protocol::Version = serde_json::from_value(json!({
                "version": version,
                "url": url,
            }))?;
        }

        let (token, url, roles_json): (String, String, String) =
            sqlx::query_as("SELECT token, url, roles_json FROM credentials")
                .fetch_one(&pool)
                .await?;
        let credential: ocpi_protocol::Credential = serde_json::from_value(json!({
            "token": token,
            "url": url,
            "roles": serde_json::from_str::<serde_json::Value>(&roles_json)?,
        }))?;
        assert_eq!(credential.roles.len(), 1);
        assert_eq!(credential.roles[0].party_id, "EXA");

        MIGRATOR.run(&pool).await?;
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM credentials")
            .fetch_one(&pool)
            .await?;
        assert_eq!(count, 1);
        Ok(())
    }

    #[tokio::test]
    async fn rejects_invalid_credential_values() -> anyhow::Result<()> {
        let pool = get_sqlite_pool().await?;
        for (token, roles) in [("x".repeat(65), "[]"), ("other-token".into(), "{}")] {
            let result =
                sqlx::query("INSERT INTO credentials (token, url, roles_json) VALUES (?, ?, ?)")
                    .bind(token)
                    .bind("https://example.com/ocpi/versions/")
                    .bind(roles)
                    .execute(&pool)
                    .await;
            assert!(result.is_err());
        }
        Ok(())
    }
}
