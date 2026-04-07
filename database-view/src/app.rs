use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::get;
use serde::Deserialize;

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

pub async fn serve(bind: SocketAddr, data_dir: PathBuf) -> Result<()> {
    let state = AppState { data_dir };
    let app = Router::new()
        .route("/", get(index))
        .route("/db", get(database_overview))
        .route("/table", get(table_page))
        .fallback(not_found)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!(
        "Database viewer running at http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;
    Ok(())
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
