use std::env;

mod api;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let clickhouse_url =
        env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
    let clickhouse_user = env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
    let clickhouse_password = env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| String::new());
    let clickhouse_db = env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "poly_dearboard".into());
    let port: u16 = env::var("API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3001);

    let client = clickhouse::Client::default()
        .with_url(&clickhouse_url)
        .with_user(&clickhouse_user)
        .with_password(&clickhouse_password)
        .with_database(&clickhouse_db);

    api::server::run(client, port).await;
}
