use std::sync::{Arc, RwLock};

use axum::{
    extract::State,
    response::Html,
    routing::{get, post},
    Form, Json, Router,
};
use crossbeam_channel::Sender;
use opcua::{server::state::ServerState, sync};

use crate::control::{Config, SharedState};

const STYLE: &str = include_str!("style.css");
const SCRIPT: &str = include_str!("script.js");
const BODY: &str = include_str!("index.html");

#[derive(Clone)]
pub struct WebState {
    pub zaber_state: Arc<RwLock<SharedState>>,
    pub tx_start_control: Sender<()>,
    pub tx_stop_control: Sender<()>,
    pub config: Arc<RwLock<Config>>,
    pub opcua_state: Arc<sync::RwLock<ServerState>>,
}

async fn handle_default() -> Html<String> {
    tracing::debug!("GET / requested");
    Html(format!(
        "
<head>
    <style>
        {}
    </style>
</head>
<body>
    {}
    <script>
        {}
    </script>
</body>
    ",
        STYLE, BODY, SCRIPT,
    ))
}

async fn handle_refresh(State(state): State<WebState>) -> Json<SharedState> {
    tracing::debug!("GET /refresh requested");
    return Json(state.zaber_state.read().unwrap().clone());
}

async fn handle_post_config(State(state): State<WebState>, Form(data): Form<Config>) {
    tracing::debug!("POST /config requested");
    let _ = state.tx_stop_control.try_send(());

    let mut config = state.config.write().unwrap();
    *config = data;
    drop(config);
}


//async fn handle_get_opcua(State(state): State<WebState>) -> Json<ServerState> {
//    tracing::debug!("GET /opcua requested");
//
//    let mut opcua = state.opcua_state.read();
//
//    //return Json(opcua);
//    //TODO(marco): Set new state
//
//    drop(opcua);
//}


async fn handle_post_opcua(State(state): State<WebState>, Form(data): Form<Config>) {
    tracing::debug!("POST /opcua requested");

    let mut opcua = state.opcua_state.write();
    opcua.abort();

    //TODO(marco): Set new state

    drop(opcua);
    dbg!("post opcua end");
}

async fn handle_post_start(State(state): State<WebState>) {
    tracing::debug!("POST start requested");
    let _ = state.tx_start_control.try_send(());
}

async fn handle_post_stop(State(state): State<WebState>) {
    tracing::debug!("POST stop requested");
    let _ = state.tx_stop_control.try_send(());
}

async fn handle_get_config(State(state): State<WebState>) -> Json<Config> {
    tracing::debug!("GET config requested");
    let config = { state.config.read().unwrap().clone() };

    Json(config)
}

pub fn run_web_server(
    state: WebState,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let app: Router<_> = Router::new()
        .route("/", get(handle_default))
        .route("/refresh", get(handle_refresh))
        .with_state(state.clone())
        .route("/config", post(handle_post_config))
        .with_state(state.clone())
        .route("/opcua", post(handle_post_opcua))
        .with_state(state.clone())
        .route("/start", post(handle_post_start))
        .with_state(state.clone())
        .route("/stop", post(handle_post_stop))
        .with_state(state.clone())
        .route("/config", get(handle_get_config))
        .with_state(state);

    tracing::info!("Starting webserver on port 8080");
    let _ = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
