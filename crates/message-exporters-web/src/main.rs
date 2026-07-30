//! Local browser-based GUI for message-exporters.
//!
//! An axum server binds a local-only port, serves a server-rendered HTML UI,
//! and opens the user's default browser to it. All work (exporting,
//! validating contacts, re-exporting, pushing to Vault) happens in-process on
//! background threads with full filesystem access, exactly like the native
//! `message-exporters-gui`; only the presentation layer differs.

mod jobs;
mod options;
mod params;
mod routes;
mod runner;
mod state;
mod views;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use tokio::net::TcpListener;

use state::AppState;

const STYLE_CSS: &str = include_str!("../assets/style.css");
const APP_JS: &str = include_str!("../assets/app.js");

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/", get(index))
        .route(
            "/contacts",
            get(routes::contacts::show).post(routes::contacts::submit),
        )
        .route(
            "/export",
            get(routes::export::show).post(routes::export::run),
        )
        .route("/export/clear", post(routes::export::clear))
        .route(
            "/reexport",
            get(routes::reexport::show).post(routes::reexport::run),
        )
        .route("/reexport/clear", post(routes::reexport::clear))
        .route("/vault", get(routes::vault::show).post(routes::vault::import))
        .route("/vault/authenticate", post(routes::vault::authenticate))
        .route("/vault/clear", post(routes::vault::clear))
        .route("/jobs/latest", get(routes::job::latest))
        .route("/jobs/{id}", get(routes::job::show))
        .route("/jobs/{id}/events", get(routes::job::events))
        .route("/jobs/{id}/cancel", post(routes::job::cancel))
        .route("/api/browse", get(routes::browse::browse))
        .route("/assets/style.css", get(style_css))
        .route("/assets/app.js", get(app_js))
        .with_state(state);

    // Bind to an OS-assigned loopback port; two copies of the app can then
    // run side by side without a fixed-port collision.
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("Could not bind a local port: {error}");
            std::process::exit(1);
        }
    };
    let local_addr = listener
        .local_addr()
        .expect("bound listener has a local address");
    let url = format!("http://{local_addr}/");

    println!("Message Exporters web GUI running at {url}");
    println!("Press Ctrl+C to stop the server.");
    if open::that(&url).is_err() {
        println!("Could not open a browser automatically — open {url} manually.");
    }

    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("Server error: {error}");
        std::process::exit(1);
    }
}

async fn index() -> Redirect {
    Redirect::to("/export")
}

async fn style_css() -> Response {
    ([(CONTENT_TYPE, "text/css; charset=utf-8")], STYLE_CSS).into_response()
}

async fn app_js() -> Response {
    (
        [(CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
        .into_response()
}
