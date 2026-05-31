use diesel::{ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, ManagerConfig};
use diesel_async::AsyncPgConnection;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use tokio_postgres::NoTls;
use tracing::info;

pub type DbPool = Pool<AsyncPgConnection>;

/// Build a connection pool (TLS when required by the URL, plain TCP for local dev).
pub async fn create_pool(database_url: String) -> Result<DbPool, Box<dyn std::error::Error + Send + Sync>> {
    let mut manager_config = ManagerConfig::default();
    manager_config.custom_setup = Box::new(establish_connection);

    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_config(
        database_url,
        manager_config,
    );

    let pool = Pool::builder().build(manager).await?;
    info!("Database connection pool ready");
    Ok(pool)
}

fn establish_connection(config: &str) -> BoxFuture<'_, ConnectionResult<AsyncPgConnection>> {
    async move {
        if use_tls(config) {
            connect_with_tls(config).await
        } else {
            connect_plain(config).await
        }
    }
    .boxed()
}

async fn connect_plain(config: &str) -> ConnectionResult<AsyncPgConnection> {
    let (client, connection) = tokio_postgres::connect(config, NoTls)
        .await
        .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

    AsyncPgConnection::try_from_client_and_connection(client, connection).await
}

async fn connect_with_tls(config: &str) -> ConnectionResult<AsyncPgConnection> {
    let rustls_config = ClientConfig::with_platform_verifier().map_err(|e| {
        ConnectionError::BadConnection(format!("TLS config error: {e}"))
    })?;
    let tls = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config);
    let (client, connection) = tokio_postgres::connect(config, tls)
        .await
        .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

    AsyncPgConnection::try_from_client_and_connection(client, connection).await
}

/// Whether the connection string requires TLS (Neon/production) vs plain (typical localhost dev).
fn use_tls(url: &str) -> bool {
    match sslmode(url) {
        Some("disable") => false,
        Some("require") | Some("verify-ca") | Some("verify-full") => true,
        // `prefer` / `allow`: do not force TLS so local Postgres without SSL still works.
        Some("prefer") | Some("allow") => false,
        None if is_local_host(url) => false,
        // Unknown or missing sslmode on remote hosts: default to TLS (Neon, etc.).
        Some(_) | None => true,
    }
}

fn sslmode(url: &str) -> Option<&str> {
    url.split(['?', '&'])
        .find_map(|part| part.strip_prefix("sslmode="))
}

fn is_local_host(url: &str) -> bool {
    ["@localhost/", "@localhost:", "@127.0.0.1/", "@127.0.0.1:"]
        .iter()
        .any(|pattern| url.contains(pattern))
}
