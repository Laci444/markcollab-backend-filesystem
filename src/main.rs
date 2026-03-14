pub mod api;
mod auth;
pub mod db;
pub mod error;

use crate::api::create_router;
use crate::db::inmemory::InMemoryRepository;
use crate::db::postresql::PostgresRepository;
use crate::db::Repository;
use opendal::services::{Fs, S3};
use opendal::Operator;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Repository>,
    pub storage: Operator,
}
#[tokio::main]
async fn main() {
    init_tracing();

    /*
    std::fs::create_dir_all("./storage").expect("Failed to create storage directory");
    let builder = Fs::default().root(".storage");
    let storage = Operator::new(builder)
        .expect("Failed to create OpenDAL operator")
        .finish();
     */
    let builder = S3::default()
        .endpoint("http://server.home:8333")
        .bucket("filesystem")
        .region("auto")
        .root("/objects")
        .access_key_id("admin")
        .secret_access_key("admin123");
    let storage = Operator::new(builder)
        .expect("Failed to create OpenDAL operator")
        .finish();

    //let db = Arc::new(InMemoryRepository::new());
    let db = {
        let pool = sqlx::PgPool::connect(&*std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        Arc::new(PostgresRepository::new(pool))
    };

    let state = AppState { db, storage };

    let app = create_router(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("Listening on http://0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "info,markcollab_backend_filesystem=debug,axum=info,tower_http=info",
        )
    });

    let formatting_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .pretty();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(formatting_layer)
        .init();

    info!("Tracing initialized");
}
