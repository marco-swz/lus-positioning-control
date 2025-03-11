use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    sync::{Arc, RwLock},
    time::Duration,
};

use anyhow::{anyhow, Result};
use axum::{
    extract::{
        self,
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Form, Json, Router,
};
use crossbeam_channel::Sender;
use futures::{SinkExt, StreamExt};
use opcua::{
    server::{config::ServerConfig, state::ServerState},
    sync,
};
use serde_json;

use crate::utils::{self, Config, ControlMode, SharedState};

const STYLE: &str = include_str!("style.css");
const SCRIPT: &str = include_str!("script.js");
const BODY: &str = include_str!("index.html");

#[derive(Clone)]
pub struct WebState {
    pub zaber_state: Arc<RwLock<SharedState>>,
    pub tx_start_control: Sender<()>,
    pub tx_stop_control: Sender<()>,
    pub target_manual: Arc<RwLock<(u32, u32, f64, f64)>>,
    pub config: Arc<RwLock<utils::Config>>,
    pub opcua_state: Arc<sync::RwLock<ServerState>>,
}

// Make our own error that wraps `anyhow::Error`.
struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

async fn handle_default(State(state): State<WebState>) -> Html<String> {
    tracing::debug!("GET / requested");
    let config = { state.config.read().unwrap() };

    Html(format!(
        "
<head>
    <script>
        var PORT = {};
        var IP_ADDR = 'localhost';
    </script>
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
        config.web_port, STYLE, BODY, SCRIPT,
    ))
}

async fn handle_refresh(State(state): State<WebState>) -> Json<SharedState> {
    tracing::debug!("GET /refresh requested");
    let state = Json(state.zaber_state.read().unwrap().clone());
    tracing::debug!("GET /refresh exit");
    return state;
}

fn save_config(config_new: &utils::Config) -> Result<()> {
    return match toml::to_string_pretty(&config_new) {
        Ok(config) => {
            match std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open("config.toml")
            {
                Ok(mut file) => match file.write_all(config.as_bytes()) {
                    Ok(_) => {
                        tracing::debug!("`config.toml` successfully written");
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!("error writing to `config.toml: {e}");
                        Err(anyhow!("error writing to `config.toml: {e}"))
                    }
                },
                Err(e) => {
                    tracing::error!("error opening `config.toml: {e}");
                    Err(anyhow!("error opening `config.toml: {e}"))
                }
            }
        }
        Err(e) => {
            tracing::error!("error serializing new config: {e}");
            Err(anyhow!("error serializing new config: {e}"))
        }
    };
}

async fn handle_post_mode(
    extract::Path(new_mode): extract::Path<ControlMode>,
    State(state): State<WebState>,
) {
    tracing::debug!("POST mode requested - new mode: {:?}", new_mode);
    let mut config_new = state.config.read().unwrap().clone();
    config_new.control_mode = new_mode.clone();

    save_config(&config_new).unwrap();
    {
        state.config.write().unwrap().control_mode = new_mode;
    }

    let _ = state.tx_stop_control.try_send(());
    tracing::debug!("POST mode exit");
}

async fn handle_post_config(
    State(state): State<WebState>,
    Form(map_new): Form<HashMap<String, String>>,
) -> Result<(), AppError> {
    tracing::debug!("POST /config requested");

    let config_new = Config {
        cycle_time_ns: Duration::from_micros(
            map_new
                .get("cycle_time_ns")
                .unwrap()
                .parse()
                .expect("Unable to parse cycle_time_ns"),
        ),
        serial_device: map_new.get("serial_device").unwrap().into(),
        opcua_config_path: map_new.get("opcua_config_path").unwrap().into(),
        control_mode: match &(map_new
            .get("control_mode")
            .ok_or(anyhow!("Missing parameter control_mode"))?[..])
        {
            "tracking" => Ok(ControlMode::Tracking),
            "manual" => Ok(ControlMode::Manual),
            _ => Err(anyhow!("Invalid control mode")),
        }?,
        limit_max_coax: map_new
            .get("limit_max_coax")
            .ok_or(anyhow!("Missing parameter limit_max_coax"))?
            .parse()
            .or(Err(anyhow!("Unable to parse limit_max_coax")))?,
        limit_min_coax: map_new
            .get("limit_min_coax")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse limit_min_coax")))?,
        maxspeed_coax: map_new
            .get("maxspeed_coax")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse maxspeed_coax")))?,
        accel_coax: map_new
            .get("accel_coax")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse accel_coax")))?,
        offset_coax: map_new
            .get("offset_coax")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse offset_coax")))?,
        limit_max_cross: map_new
            .get("limit_max_cross")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse limit_max_cross")))?,
        limit_min_cross: map_new
            .get("limit_min_cross")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse limit_min_cross")))?,
        maxspeed_cross: map_new
            .get("maxspeed_cross")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse maxspeed_cross")))?,
        accel_cross: map_new
            .get("accel_cross")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse accel_cross")))?,
        mock_zaber: map_new
            .get("mock_zaber")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse mock_zaber")))?,
        formula_coax: map_new
            .get("formula_coax")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse formula_coax")))?,
        formula_cross: map_new
            .get("formula_cross")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse formula_cross")))?,
        web_port: map_new
            .get("web_port")
            .unwrap()
            .parse()
            .or(Err(anyhow!("Unable to parse web_port")))?,
    };

    let _ = state.tx_stop_control.try_send(());

    let save_result = save_config(&config_new);

    let mut config = state.config.write().unwrap();
    *config = config_new;
    drop(config);

    save_result?;
    Ok(())
}

async fn handle_get_opcua(State(state): State<WebState>) -> Json<ServerConfig> {
    tracing::debug!("GET /opcua requested");

    let opcua = state.opcua_state.read();

    return Json(opcua.config.read().clone());
}

async fn handle_post_opcua(State(state): State<WebState>, Form(new_config): Form<ServerConfig>) {
    tracing::debug!("POST /opcua requested");
    if !opcua::core::config::Config::is_valid(&new_config) {
        tracing::error!("new opcua config is invalid");
        Err(anyhow!("new opcua config is invalid")).unwrap()
    }

    let config_path = { state.config.read().unwrap().opcua_config_path.clone() };

    match opcua::core::config::Config::save(&new_config, Path::new(&config_path)) {
        Ok(_) => tracing::debug!("successfully saved new opcua config"),
        Err(_) => {
            tracing::error!("failed to write to opcua config")
        }
    }

    let mut opcua = state.opcua_state.write();
    opcua.abort();
    tracing::debug!("opcua server aborted");
    drop(opcua);
}

async fn handle_post_start(State(state): State<WebState>) {
    tracing::debug!("POST start requested");
    let _ = state.tx_start_control.try_send(());
    tracing::debug!("POST start exit");
}

async fn handle_post_stop(State(state): State<WebState>) {
    tracing::debug!("POST stop requested");
    let _ = state.tx_stop_control.try_send(());
    tracing::debug!("POST stop exit");
}

async fn handle_get_config(State(state): State<WebState>) -> Json<utils::Config> {
    tracing::debug!("GET config requested");
    let config = { state.config.read().unwrap().clone() };

    Json(config)
}

async fn handle_manual_init(
    ws: WebSocketUpgrade,
    State(state): State<WebState>,
) -> impl IntoResponse {
    tracing::debug!("Manual init");
    ws.on_upgrade(move |socket| handle_manual(socket, state))
}

fn parse_message(msg: Message) -> Result<(u32, u32)> {
    let msg = msg.to_text()?;
    let mut msg = msg.split_whitespace();

    let val_coax = str::parse::<u32>(msg.next().ok_or(anyhow!("Missing value"))?)?;
    let val_cross = str::parse::<u32>(msg.next().ok_or(anyhow!("Missing value"))?)?;

    return Ok((val_coax, val_cross));
}

async fn handle_manual(socket: WebSocket, state: WebState) {
    let (mut sender, mut receiver) = socket.split();

    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            let msg = if let Ok(msg) = msg {
                msg
            } else {
                return; // client disconnected
            };

            let (val_coax, val_cross) = match parse_message(msg) {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!("Error parsing message: {e}");
                    continue;
                }
            };

            {
                match state.target_manual.write() {
                    Err(e) => tracing::error!("Failed to aquire manual voltage lock: {e}"),
                    Ok(mut v) => *v = (val_coax, val_cross, 0., 0.),
                };
            }
        }
    });

    let mut send_task = tokio::spawn(async move {
        loop {
            let state = { state.zaber_state.read().unwrap().clone() };

            let state_json = serde_json::to_string(&state).unwrap();
            sender.send(Message::Text(state_json)).await.unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });

    // If any one of the tasks exit, abort the other.
    tokio::select! {
        rv_a = (&mut send_task) => {
            match rv_a {
                Ok(_) => (),
                Err(a) => tracing::error!("Error sending messages {a:?}")
            }
            recv_task.abort();
        },
        rv_b = (&mut recv_task) => {
            match rv_b {
                Ok(_) => (),
                Err(b) => tracing::error!("Error receiving messages {b:?}")
            }
            send_task.abort();
        }
    }
}

pub fn run_web_server(state: WebState) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let config = { state.config.read().unwrap().clone() };

    let app: Router<_> = Router::new()
        .route("/", get(handle_default))
        .with_state(state.clone())
        .route("/refresh", get(handle_refresh))
        .with_state(state.clone())
        .route("/config", post(handle_post_config))
        .with_state(state.clone())
        .route("/opcua", get(handle_get_opcua))
        .with_state(state.clone())
        .route("/opcua", post(handle_post_opcua))
        .with_state(state.clone())
        .route("/start", post(handle_post_start))
        .with_state(state.clone())
        .route("/stop", post(handle_post_stop))
        .with_state(state.clone())
        .route("/config", get(handle_get_config))
        .with_state(state.clone())
        .route("/mode/:m", post(handle_post_mode))
        .with_state(state.clone())
        .route("/ws", get(handle_manual_init))
        .with_state(state);

    tracing::info!("Starting webserver on port {}", config.web_port);
    let _ = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.web_port))
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
