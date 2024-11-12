use std::sync::{Arc, RwLock};

use axum::{extract::State, response::Html, routing::{get, post}, Form, Json, Router};
use crossbeam_channel::Sender;

use crate::control::{Config, ControlState, SharedState, StateChannel};
type AppState = (Arc<RwLock<SharedState>>, Sender<()>, Sender<()>, Arc<RwLock<Config>>);

const STYLE: &str = include_str!("style.css");
const SCRIPT: &str = include_str!("script.js");
const BODY: &str = include_str!("index.html");

async fn handle_default() -> Html<String> {
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
        STYLE,
        BODY,
        SCRIPT,
    ))
}

async fn handle_refresh(State(state): State<AppState>) -> Json<SharedState> {
    dbg!("get refresh");
    return Json(state.0.read().unwrap().clone());
}

async fn handle_post_config(State(state): State<AppState>, Form(data): Form<Config>) {
    dbg!("post config begin");
    let _ = state.2.try_send(());

    let mut config = state.3.write().unwrap();
    *config = data;
    drop(config);
    dbg!("post config end");
}

async fn handle_post_start(State(state): State<AppState>) {
    dbg!("post start begin");
    let _ = state.1.try_send(());
    dbg!("post start end");
}

async fn handle_post_stop(State(state): State<AppState>) {
    dbg!("post stop begin");
    let _ = state.2.try_send(());
    dbg!("post stop end");
}

async fn handle_get_config(State(state): State<AppState>) -> Json<Config> {
    dbg!("get config begin");
    let config = {
        state.3.read().unwrap().clone()
    };

    dbg!("get config end");
    Json(config)
}



pub fn run_web_server(zaber_state: StateChannel, tx_start: Sender<()>, tx_stop: Sender<()>, config: Arc<RwLock<Config>>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let shared_state = (zaber_state, tx_start, tx_stop, config);
    let app: Router<_> = Router::new()
        .route("/", get(handle_default))
        .route("/refresh", get(handle_refresh)).with_state(shared_state.clone())
        .route("/config", post(handle_post_config)).with_state(shared_state.clone())
        .route("/start", post(handle_post_start)).with_state(shared_state.clone())
        .route("/stop", post(handle_post_stop)).with_state(shared_state.clone())
        .route("/config", get(handle_get_config)).with_state(shared_state);

    let _ = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
