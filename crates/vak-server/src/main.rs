mod assets;
mod config;
mod routes;
mod security;
mod state;

use std::net::SocketAddr;

use axum::middleware;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=warn".into())).init();
    let cfg = config::Config::from_env();
    if cfg.sarvam_key.is_empty() {
        tracing::warn!("SARVAM_API_KEY is not set: visitors will need to bring their own key");
    }
    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port).parse().expect("host and port form an address");
    let origin = (!cfg.supabase_url.is_empty()).then(|| cfg.supabase_url.trim_end_matches('/').to_string());
    let state = state::AppState::new(cfg);
    let app = routes::router(state)
        .fallback(get(assets::serve))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(move |req, next| security::headers(origin.clone(), req, next)))
        .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(addr).await.expect("port is free");
    tracing::info!("Vak is listening on http://{addr}");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server runs");
}
