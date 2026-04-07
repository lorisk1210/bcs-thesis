use axum::http::StatusCode;
use serde::Serialize;

use crate::models::{DatabaseListPage, DatabaseOverview, TablePage};

pub fn render_index(page: &DatabaseListPage) -> String {
    let body = if page.databases.is_empty() {
        format!(
            r#"
            <section class="panel">
              <h2>No DuckDB files found</h2>
              <p>No <code>.duckdb</code> files were found under <code>{}</code>.</p>
            </section>
            "#,
            escape_html(&page.data_dir_display)
        )
    } else {
        let rows = page
            .databases
            .iter()
            .map(|database| {
                let open_link = db_link(&database.relative_path);
                let relation_count = database
                    .relation_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "n/a".to_string());
                let status = database
                    .inspection_error
                    .as_ref()
                    .map(|error| {
                        format!(
                            r#"<span class="badge badge-warn" title="{}">inspection failed</span>"#,
                            escape_html(error)
                        )
                    })
                    .unwrap_or_else(|| r#"<span class="badge">ok</span>"#.to_string());
                format!(
                    r#"
                    <tr>
                      <td><a href="{open_link}"><strong>{file_name}</strong></a><br><span class="muted mono">{relative_path}</span></td>
                      <td>{kind}</td>
                      <td>{size}</td>
                      <td>{modified}</td>
                      <td>{relation_count}</td>
                      <td>{status}</td>
                    </tr>
                    "#,
                    file_name = escape_html(&database.file_name),
                    relative_path = escape_html(&database.relative_path),
                    kind = escape_html(database.kind.label()),
                    size = escape_html(&format_bytes(database.size_bytes)),
                    modified = escape_html(&format_datetime(database.modified_at)),
                )
            })
            .collect::<String>();

        format!(
            r#"
            <section class="panel">
              <h2>DuckDB files in <code>{}</code></h2>
              <div class="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>Database</th>
                      <th>Type</th>
                      <th>Size</th>
                      <th>Modified</th>
                      <th>Relations</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>{rows}</tbody>
                </table>
              </div>
            </section>
            "#,
            escape_html(&page.data_dir_display)
        )
    };

    render_page(
        "Refinery Database Viewer",
        "Read-only local browser for DuckDB files under data/.",
        &body,
    )
}

pub fn render_database_overview(page: &DatabaseOverview) -> String {
    let cards = page
        .cards
        .iter()
        .map(|card| {
            format!(
                r#"
                <article class="card">
                  <div class="card-label">{}</div>
                  <div class="card-value">{}</div>
                </article>
                "#,
                escape_html(&card.label),
                escape_html(&card.value)
            )
        })
        .collect::<String>();

    let relation_rows = page
        .relations
        .iter()
        .map(|relation| {
            let link = table_link(&page.relative_path, &relation.name, 1);
            format!(
                r#"
                <tr>
                  <td><a href="{link}">{name}</a></td>
                  <td>{relation_kind}</td>
                  <td><span class="badge">{category}</span></td>
                  <td>{row_count}</td>
                  <td>{column_count}</td>
                </tr>
                "#,
                name = escape_html(&relation.name),
                relation_kind = escape_html(relation.relation_kind.label()),
                category = escape_html(relation.category.label()),
                row_count = relation.row_count,
                column_count = relation.column_count,
            )
        })
        .collect::<String>();

    let body = format!(
        r#"
        <nav class="breadcrumbs">
          <a href="/">Databases</a>
        </nav>
        <section class="panel">
          <h1>{file_name}</h1>
          <p class="muted mono">{relative_path}</p>
          <dl class="meta-grid">
            <div><dt>Type</dt><dd>{kind}</dd></div>
            <div><dt>Size</dt><dd>{size}</dd></div>
            <div><dt>Modified</dt><dd>{modified}</dd></div>
            <div><dt>Mode</dt><dd>Read-only</dd></div>
          </dl>
        </section>
        <section class="cards">{cards}</section>
        <section class="panel">
          <h2>Relations</h2>
          <div class="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Kind</th>
                  <th>Category</th>
                  <th>Rows</th>
                  <th>Columns</th>
                </tr>
              </thead>
              <tbody>{relation_rows}</tbody>
            </table>
          </div>
        </section>
        "#,
        file_name = escape_html(&page.file_name),
        relative_path = escape_html(&page.relative_path),
        kind = escape_html(page.kind.label()),
        size = escape_html(&format_bytes(page.size_bytes)),
        modified = escape_html(&format_datetime(page.modified_at)),
    );

    render_page(
        &format!("{} · Database Viewer", page.file_name),
        "Overview of tables, views, and summary counts.",
        &body,
    )
}

pub fn render_table_page(page: &TablePage) -> String {
    let schema_rows = page
        .columns
        .iter()
        .map(|column| {
            format!(
                r#"
                <tr>
                  <td class="mono">{}</td>
                  <td class="mono">{}</td>
                  <td>{}</td>
                </tr>
                "#,
                escape_html(&column.name),
                escape_html(&column.data_type),
                if column.nullable { "yes" } else { "no" }
            )
        })
        .collect::<String>();

    let header_cells = page
        .columns
        .iter()
        .map(|column| format!("<th>{}</th>", escape_html(&column.name)))
        .collect::<String>();

    let row_cells = if page.rows.is_empty() {
        format!(
            r#"<tr><td colspan="{}" class="muted">No rows on this page.</td></tr>"#,
            page.columns.len().max(1)
        )
    } else {
        page.rows
            .iter()
            .map(|row| {
                let cells = row
                    .iter()
                    .map(|cell| format!("<td>{}</td>", escape_html(cell)))
                    .collect::<String>();
                format!("<tr>{cells}</tr>")
            })
            .collect::<String>()
    };

    let previous_link = if page.has_previous_page {
        format!(
            r#"<a class="button" href="{}">Previous page</a>"#,
            table_link(
                &page.database_relative_path,
                &page.relation_name,
                page.page.saturating_sub(1),
            )
        )
    } else {
        r#"<span class="button button-disabled">Previous page</span>"#.to_string()
    };
    let next_link = if page.has_next_page {
        format!(
            r#"<a class="button" href="{}">Next page</a>"#,
            table_link(
                &page.database_relative_path,
                &page.relation_name,
                page.page + 1
            )
        )
    } else {
        r#"<span class="button button-disabled">Next page</span>"#.to_string()
    };

    let body = format!(
        r#"
        <nav class="breadcrumbs">
          <a href="/">Databases</a>
          <span>/</span>
          <a href="{db_link}">{db_name}</a>
        </nav>
        <section class="panel">
          <h1>{relation_name}</h1>
          <p class="muted mono">{database_relative_path}</p>
          <dl class="meta-grid">
            <div><dt>Database type</dt><dd>{database_kind}</dd></div>
            <div><dt>Relation kind</dt><dd>{relation_kind}</dd></div>
            <div><dt>Category</dt><dd>{category}</dd></div>
            <div><dt>Total rows</dt><dd>{total_rows}</dd></div>
          </dl>
        </section>
        <section class="panel">
          <h2>Schema</h2>
          <div class="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Column</th>
                  <th>Type</th>
                  <th>Nullable</th>
                </tr>
              </thead>
              <tbody>{schema_rows}</tbody>
            </table>
          </div>
        </section>
        <section class="panel">
          <div class="panel-head">
            <h2>Rows</h2>
            <div class="muted">Page {page_number} · {page_size} rows per page</div>
          </div>
          <div class="table-scroll">
            <table class="data-table">
              <thead>
                <tr>{header_cells}</tr>
              </thead>
              <tbody>{row_cells}</tbody>
            </table>
          </div>
          <div class="pager">{previous_link}{next_link}</div>
        </section>
        "#,
        db_link = db_link(&page.database_relative_path),
        db_name = escape_html(&page.database_file_name),
        relation_name = escape_html(&page.relation_name),
        database_relative_path = escape_html(&page.database_relative_path),
        database_kind = escape_html(page.database_kind.label()),
        relation_kind = escape_html(page.relation_kind.label()),
        category = escape_html(page.category.label()),
        total_rows = page.total_rows,
        page_number = page.page,
        page_size = page.page_size,
    );

    render_page(
        &format!("{} · {}", page.database_file_name, page.relation_name),
        "Schema and paginated sample rows.",
        &body,
    )
}

pub fn render_error_page(title: &str, message: &str, status: StatusCode) -> String {
    let body = format!(
        r#"
        <nav class="breadcrumbs">
          <a href="/">Databases</a>
        </nav>
        <section class="panel">
          <h1>{}</h1>
          <p class="muted">HTTP {}</p>
          <p>{}</p>
        </section>
        "#,
        escape_html(title),
        status.as_u16(),
        escape_html(message)
    );

    render_page(title, "Local database viewer error page.", &body)
}

fn render_page(title: &str, subtitle: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title}</title>
    <style>
      :root {{
        color-scheme: light;
        --bg: #f6f3ec;
        --panel: #fffdf9;
        --ink: #1d1b19;
        --muted: #6d675e;
        --line: #d9d0c3;
        --accent: #9f2f1f;
        --accent-soft: #f6e0d9;
      }}
      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        background: var(--bg);
        color: var(--ink);
        font-family: "Avenir Next", "Helvetica Neue", sans-serif;
        line-height: 1.45;
      }}
      .shell {{
        max-width: 1120px;
        margin: 0 auto;
        padding: 28px 20px 48px;
      }}
      header {{
        margin-bottom: 24px;
      }}
      h1, h2 {{
        font-family: "Iowan Old Style", "Palatino Linotype", serif;
        line-height: 1.15;
        margin: 0 0 10px;
      }}
      h1 {{ font-size: 2rem; }}
      h2 {{ font-size: 1.25rem; }}
      p {{ margin: 0 0 12px; }}
      a {{
        color: var(--accent);
        text-decoration: none;
      }}
      a:hover {{ text-decoration: underline; }}
      code, .mono {{
        font-family: "SF Mono", "Monaco", "Cascadia Code", monospace;
        font-size: 0.95em;
      }}
      .lead {{
        max-width: 68ch;
        color: var(--muted);
      }}
      .panel {{
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: 14px;
        padding: 18px;
        margin-bottom: 18px;
        box-shadow: 0 1px 0 rgba(0, 0, 0, 0.02);
      }}
      .cards {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: 12px;
        margin-bottom: 18px;
      }}
      .card {{
        background: var(--panel);
        border: 1px solid var(--line);
        border-radius: 14px;
        padding: 16px;
      }}
      .card-label {{
        color: var(--muted);
        font-size: 0.9rem;
        margin-bottom: 8px;
      }}
      .card-value {{
        font-size: 1.5rem;
        font-weight: 700;
      }}
      .badge {{
        display: inline-block;
        padding: 3px 8px;
        border-radius: 999px;
        background: #efe8dc;
        border: 1px solid var(--line);
        color: #4a433b;
        font-size: 0.8rem;
      }}
      .badge-warn {{
        background: var(--accent-soft);
        border-color: #e7b6a8;
        color: #7e2316;
      }}
      table {{
        border-collapse: collapse;
        font-size: 0.95rem;
        min-width: 100%;
      }}
      .table-scroll {{
        overflow-x: auto;
        overflow-y: hidden;
        -webkit-overflow-scrolling: touch;
        padding-bottom: 4px;
      }}
      .table-scroll > table {{
        width: max-content;
      }}
      .data-table td,
      .data-table th {{
        white-space: nowrap;
      }}
      th, td {{
        text-align: left;
        padding: 10px 12px;
        vertical-align: top;
        border-top: 1px solid var(--line);
        overflow-wrap: anywhere;
      }}
      thead th {{
        border-top: none;
        color: var(--muted);
        font-weight: 600;
      }}
      .meta-grid {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: 12px;
        margin: 0;
      }}
      .meta-grid div {{
        padding: 10px 12px;
        background: #faf6ef;
        border: 1px solid var(--line);
        border-radius: 10px;
      }}
      dt {{
        color: var(--muted);
        font-size: 0.85rem;
        margin-bottom: 6px;
      }}
      dd {{
        margin: 0;
        font-weight: 600;
      }}
      .muted {{
        color: var(--muted);
      }}
      .breadcrumbs {{
        display: flex;
        gap: 8px;
        align-items: center;
        margin: 0 0 16px;
        color: var(--muted);
        font-size: 0.95rem;
      }}
      .panel-head {{
        display: flex;
        justify-content: space-between;
        gap: 12px;
        align-items: baseline;
        margin-bottom: 12px;
      }}
      .pager {{
        display: flex;
        gap: 10px;
        margin-top: 14px;
      }}
      .button {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 8px 12px;
        border-radius: 10px;
        border: 1px solid var(--line);
        background: #faf6ef;
      }}
      .button-disabled {{
        color: var(--muted);
      }}
      @media (max-width: 720px) {{
        .shell {{
          padding: 20px 14px 36px;
        }}
        th, td {{
          padding: 8px 10px;
        }}
        .panel-head {{
          flex-direction: column;
          align-items: flex-start;
        }}
      }}
    </style>
  </head>
  <body>
    <div class="shell">
      <header>
        <h1>{heading}</h1>
        <p class="lead">{subtitle}</p>
      </header>
      {body}
    </div>
  </body>
</html>"#,
        title = escape_html(title),
        heading = escape_html(title),
        subtitle = escape_html(subtitle),
        body = body,
    )
}

fn db_link(database_relative_path: &str) -> String {
    let query = serde_urlencoded::to_string(DatabaseQuery {
        db: database_relative_path,
    })
    .expect("database query should serialize");
    format!("/db?{query}")
}

fn table_link(database_relative_path: &str, relation_name: &str, page: usize) -> String {
    let query = serde_urlencoded::to_string(TableQuery {
        db: database_relative_path,
        table: relation_name,
        page,
    })
    .expect("table query should serialize");
    format!("/table?{query}")
}

fn format_datetime(datetime: Option<chrono::DateTime<chrono::Local>>) -> String {
    datetime
        .map(|datetime| datetime.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_bytes(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size_bytes as f64;
    let mut unit_index = 0;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{size_bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[derive(Serialize)]
struct DatabaseQuery<'a> {
    db: &'a str,
}

#[derive(Serialize)]
struct TableQuery<'a> {
    db: &'a str,
    table: &'a str,
    page: usize,
}
