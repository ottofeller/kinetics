use http::{Request, Response};
use kinetics::tools::config::Config;
use kinetics::{macros::endpoint, tools::http::Body};
use serde_json::json;
use std::collections::HashMap;
// As an example use a general-purpose type-erased error from tower.
// Custom errors would work as well.
use tower::BoxError;

/// Interact with Sql DB
///
/// Create a record, then query it from DB, and return the result in http response.
///
/// Test locally with the following command:
/// kinetics invoke DatabaseDatabase --with-db
#[endpoint(url_path = "/database")]
pub async fn database(
    _event: Request<Body>,
    _secrets: &HashMap<String, String>,
    config: &Config,
) -> Result<Response<String>, BoxError> {
    // Connect to the database using sqlx crate and connection string
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&config.db.connection_string())
        .await?;

    // Create a table if it doesn't exist
    sqlx::query(r#"CREATE TABLE IF NOT EXISTS example (value SMALLINT NOT NULL)"#)
        .execute(&pool)
        .await?;

    // Insert a value into the table
    sqlx::query(r#"INSERT INTO example (value) VALUES (1)"#)
        .execute(&pool)
        .await?;

    // Read values from the table
    let result = sqlx::query_scalar::<_, i16>(r#"SELECT value FROM "example" LIMIT 10"#)
        .fetch_all(&pool)
        .await?;

    let resp = Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(json!({"values": result}).to_string())?;

    Ok(resp)
}
