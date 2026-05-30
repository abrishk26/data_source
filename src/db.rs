use diesel::{ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, ManagerConfig};
use diesel_async::AsyncPgConnection;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use tracing::info;

pub type DbPool = Pool<AsyncPgConnection>;

/// Build a connection pool with TLS (required for hosted Postgres e.g. Neon).
pub async fn create_pool(database_url: String) -> Result<DbPool, Box<dyn std::error::Error + Send + Sync>> {
    let mut manager_config = ManagerConfig::default();
    manager_config.custom_setup = Box::new(establish_tls_connection);

    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_config(
        database_url,
        manager_config,
    );

    let pool = Pool::builder().build(manager).await?;
    info!("Database connection pool ready");
    Ok(pool)
}

fn establish_tls_connection(config: &str) -> BoxFuture<'_, ConnectionResult<AsyncPgConnection>> {
    async move {
        let rustls_config = ClientConfig::with_platform_verifier().map_err(|e| {
            ConnectionError::BadConnection(format!("TLS config error: {e}"))
        })?;
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config);
        let (client, connection) = tokio_postgres::connect(config, tls)
            .await
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

        AsyncPgConnection::try_from_client_and_connection(client, connection).await
    }
    .boxed()
}
