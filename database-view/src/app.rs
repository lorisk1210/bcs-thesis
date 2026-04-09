use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::get;
use cli_render::{
    DatabaseViewStartedData, OutputMode, overwrite_service_render, render_database_view_started,
    render_database_view_stopped,
};
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::db;
use crate::error::{ViewerError, ViewerResult};

#[derive(Debug, Clone)]
struct AppState {
    data_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct DatabaseQuery {
    db: String,
}

#[derive(Debug, Deserialize)]
struct TableQuery {
    db: String,
    table: String,
    page: Option<usize>,
}

pub async fn serve(mode: OutputMode, bind: SocketAddr, data_dir: PathBuf) -> Result<()> {
    let data_dir_display = data_dir.display().to_string();
    let state = AppState { data_dir };
    let app = Router::new()
        .route("/", get(index))
        .route("/db", get(database_overview))
        .route("/table", get(table_page))
        .fallback(not_found)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    let bind_addr = listener.local_addr()?.to_string();
    let started_output = render_database_view_started(
        mode,
        &DatabaseViewStartedData {
            bind_addr: bind_addr.clone(),
            data_dir: data_dir_display,
            browser_url: format!("http://{bind_addr}/"),
        },
    );
    print!("{started_output}");
    let stopped_by_signal = Arc::new(AtomicBool::new(false));
    let shutdown_seen = Arc::clone(&stopped_by_signal);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown_seen.store(true, Ordering::SeqCst);
        })
        .await?;
    if stopped_by_signal.load(Ordering::SeqCst) {
        let stopped_output = render_database_view_stopped(mode, &bind_addr);
        if mode == OutputMode::Pretty {
            print!(
                "{}",
                overwrite_service_render(&started_output, &stopped_output)
            );
        } else {
            print!("{stopped_output}");
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn index(State(state): State<AppState>) -> ViewerResult<Html<String>> {
    let data_dir = state.data_dir.clone();
    let page = tokio::task::spawn_blocking(move || db::list_databases(&data_dir))
        .await
        .map_err(ViewerError::from_join)??;
    Ok(Html(crate::html::render_index(&page)))
}

async fn database_overview(
    State(state): State<AppState>,
    Query(query): Query<DatabaseQuery>,
) -> ViewerResult<Html<String>> {
    let data_dir = state.data_dir.clone();
    let requested_db = query.db;
    let page =
        tokio::task::spawn_blocking(move || db::load_database_overview(&data_dir, &requested_db))
            .await
            .map_err(ViewerError::from_join)??;
    Ok(Html(crate::html::render_database_overview(&page)))
}

async fn table_page(
    State(state): State<AppState>,
    Query(query): Query<TableQuery>,
) -> ViewerResult<Html<String>> {
    let data_dir = state.data_dir.clone();
    let requested_db = query.db;
    let relation_name = query.table;
    let page_number = query.page.unwrap_or(1);
    let page = tokio::task::spawn_blocking(move || {
        db::load_table_page(&data_dir, &requested_db, &relation_name, page_number)
    })
    .await
    .map_err(ViewerError::from_join)??;
    Ok(Html(crate::html::render_table_page(&page)))
}

async fn not_found() -> (StatusCode, Html<String>) {
    let status = StatusCode::NOT_FOUND;
    let body = crate::html::render_error_page("Not Found", "That page does not exist.", status);
    (status, Html(body))
}
