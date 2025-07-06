use std::{
    collections::HashMap,
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
use ftdi_embedded_hal::libftd2xx;
use futures::{SinkExt, StreamExt};
use serde_json;

use crate::utils::{self, write_config, Config, ControlMode, ControlStatus, SharedState};

const STYLE: &str = include_str!("style.css");
const SCRIPT: &str = include_str!("script.js");
const BODY: &str = include_str!("index.html");

#[derive(Clone)]
pub struct WebState {
    pub zaber_state: Arc<RwLock<SharedState>>,
    pub tx_start_control: Sender<()>,
    pub tx_stop_control: Sender<()>,
    pub target_manual: Arc<RwLock<[u32; 2]>>,
    pub config: Arc<RwLock<utils::Config>>,
}

// Make our own error that wraps `anyhow::Error`.
struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", self.0)).into_response()
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

async fn handle_post_mode(
    extract::Path(new_mode): extract::Path<ControlMode>,
    State(state): State<WebState>,
) {
    tracing::debug!("POST mode requested - new mode: {:?}", new_mode);
    let mut config_new = state.config.read().unwrap().clone();
    config_new.control_mode = new_mode.clone();

    write_config(&config_new).unwrap();
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

    if state.zaber_state.read().unwrap().control_state != ControlStatus::Stopped {
        Err(anyhow!(
            "The config cannot be changed while running. Stop the control first!"
        ))?;
    }

    let config_new = Config {
        cycle_time_ms: Duration::from_millis(
            map_new
                .get("cycle_time_ms")
                .unwrap()
                .parse()
                .expect("cycle_time_ms: Unable to parse cycle_time_ms"),
        ),
        serial_device: map_new.get("serial_device").unwrap().into(),
        opcua_config_path: map_new.get("opcua_config_path").unwrap().into(),
        control_mode: match &(map_new
            .get("control_mode")
            .ok_or(anyhow!("control_mode: Missing parameter control_mode"))?[..])
        {
            "Tracking" => Ok(ControlMode::Tracking),
            "Manual" => Ok(ControlMode::Manual),
            _ => Err(anyhow!("control_mode: Invalid control mode")),
        }?,
        limit_max_coax: map_new
            .get("limit_max_coax")
            .ok_or(anyhow!("limit_max_coax: Missing parameter limit_max_coax"))?
            .parse()
            .or(Err(anyhow!(
                "limit_max_coax: Unable to parse limit_max_coax"
            )))?,
        limit_min_coax: map_new
            .get("limit_min_coax")
            .ok_or(anyhow!("limit_min_coax: Missing parameter limit_min_coax"))?
            .parse()
            .or(Err(anyhow!(
                "limit_min_coax: Unable to parse limit_min_coax"
            )))?,
        maxspeed_coax: map_new
            .get("maxspeed_coax")
            .ok_or(anyhow!("maxspeed_coax: Missing parameter maxspeed_coax"))?
            .parse()
            .or(Err(anyhow!("maxspeed_coax: Unable to parse maxspeed_coax")))?,
        accel_coax: map_new
            .get("accel_coax")
            .ok_or(anyhow!("accel_coax: Missing parameter accel_coax"))?
            .parse()
            .or(Err(anyhow!("accel_coax: Unable to parse accel_coax")))?,
        offset_coax: map_new
            .get("offset_coax")
            .ok_or(anyhow!("offset_coax: Missing parameter offset_coax"))?
            .parse()
            .or(Err(anyhow!("offset_coax: Unable to parse offset_coax")))?,
        limit_max_cross: map_new
            .get("limit_max_cross")
            .ok_or(anyhow!("limit_max_cross: Missing parameter limit_max_cross"))?
            .parse()
            .or(Err(anyhow!(
                "limit_max_cross: Unable to parse limit_max_cross"
            )))?,
        limit_min_cross: map_new
            .get("limit_min_cross")
            .ok_or(anyhow!("limit_min_cross: Missing parameter limit_min_cross"))?
            .parse()
            .or(Err(anyhow!(
                "limit_min_cross: Unable to parse limit_min_cross"
            )))?,
        maxspeed_cross: map_new
            .get("maxspeed_cross")
            .ok_or(anyhow!("maxspeed_cross: Missing parameter maxspeed_cross"))?
            .parse()
            .or(Err(anyhow!(
                "maxspeed_cross: Unable to parse maxspeed_cross"
            )))?,
        accel_cross: map_new
            .get("accel_cross")
            .ok_or(anyhow!("accel_cross: Missing parameter accel_cross"))?
            .parse()
            .or(Err(anyhow!("accel_cross: Unable to parse accel_cross")))?,
        mock_zaber: map_new
            .get("mock_zaber")
            .ok_or(anyhow!("mock_zaber: Missing parameter mock_zaber"))?
            .parse()
            .or(Err(anyhow!("mock_zaber: Unable to parse mock_zaber")))?,
        mock_adc: map_new
            .get("mock_adc")
            .ok_or(anyhow!("mock_adc: Missing parameter mock_adc"))?
            .parse()
            .or(Err(anyhow!("mock_adc: Unable to parse mock_adc")))?,
        formula_coax: map_new
            .get("formula_coax")
            .ok_or(anyhow!("formula_coax: Missing parameter formula_coax"))?
            .parse()
            .or(Err(anyhow!("formula_coax: Unable to parse formula_coax")))?,
        formula_cross: map_new
            .get("formula_cross")
            .ok_or(anyhow!("formula_cross: Missing parameter formula_cross"))?
            .parse()
            .or(Err(anyhow!("formula_cross: Unable to parse formula_cross")))?,
        web_port: map_new
            .get("web_port")
            .ok_or(anyhow!("web_port: Missing parameter web_port"))?
            .parse()
            .or(Err(anyhow!("web_port: Unable to parse web_port")))?,
        adc_serial_number1: map_new
            .get("adc_serial_number1")
            .ok_or(anyhow!("adc_serial_number1: Missing parameter adc_serial_number1"))?
            .parse()
            .or(Err(anyhow!("adc_serial_number1: Unable to parse adc_serial_number1")))?,
        adc_serial_number2: map_new
            .get("adc_serial_number2")
            .ok_or(anyhow!("adc_serial_number2: Missing parameter adc_serial_number2"))?
            .parse()
            .or(Err(anyhow!("adc_serial_number2: Unable to parse adc_serial_number2")))?,
    };

    // If the user changes the config twice without starting
    // in between, the stop channel would be full and this call
    // errors, which doesn't matter.
    let _ = state.tx_stop_control.try_send(());

    let save_result = write_config(&config_new);

    let mut config = state.config.write().unwrap();
    *config = config_new;
    drop(config);

    save_result?;
    Ok(())
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
                    Ok(mut v) => *v = [val_coax, val_cross],
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

async fn handle_get_adc_devices(State(state): State<WebState>) -> Result<Json<Vec<String>>, AppError> {
    if state.zaber_state.read().unwrap().control_state != ControlStatus::Stopped {
        Err(anyhow!(
            "The ADC cannot be discovered while the control is running!"
        ))?;
    }

    let devices = libftd2xx::list_devices()?;
    dbg!(&devices);
    Ok(Json(devices.into_iter().map(|dev| dev.serial_number).collect()))
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
        .route("/start", post(handle_post_start))
        .with_state(state.clone())
        .route("/stop", post(handle_post_stop))
        .with_state(state.clone())
        .route("/config", get(handle_get_config))
        .with_state(state.clone())
        .route("/adc-devices", get(handle_get_adc_devices))
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
