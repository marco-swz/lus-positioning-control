use std::sync::{Arc, RwLock};

use axum::{extract::State, response::Html, routing::{get, post}, Json, Router};
use crossbeam_channel::Sender;

use crate::control::{Config, SharedState, StateChannel};
type AppState = (Arc<RwLock<SharedState>>, Sender<()>, Sender<()>, Config);

async fn handle_default(State(state): State<AppState>) -> Html<String> {
    let config = state.3;
    let shared_state = state.0;
    let shared_state = shared_state.read().unwrap();
    let state = shared_state.clone();
    drop(shared_state);

    // TODO(marco): Use `include_str!()`
    Html(format!(
        "
<head>
    <style>
        #main {{
            position: absolute;
            left: 50%;
            transform: translateX(-50%);
            margin-top: 30px;
        }}

        .content {{
            width: 400px;
            display: none;
            border: 1px solid grey;
            padding: 10px;
            box-shadow: rgba(0, 0, 0, 0.25) 0px 14px 28px, rgba(0, 0, 0, 0.22) 0px 10px 10px;
            border-radius: 4px;
            border-top-left-radius: 0;
        }}

        .grid {{
            display: grid;
            grid-template-columns: max-content 1fr;
            gap: 5px 20px;
        }}

        .visible {{
            display: block;
        }}

        #tabs {{
            display: flex;
        }}

        .tab {{
            border: 1px solid grey;
            color: grey;
            background-color: whitesmoke;
            padding: 5px;
            margin-bottom: -1px;
            cursor: pointer;
            border-top-left-radius: 4px;
            border-top-right-radius: 4px;
        }}

        .tab.active {{
            border-bottom-color: transparent;
            color: black;
            background-color: white;
        }}

        #btn-refresh {{
            margin-top: 15px;
        }}
    </style>
</head>
<body>
    <div id=\"main\">
        <div id=\"tabs\">
            <div id=\"tab-control\" class=\"tab active\" onclick=\"handleClickTab('control')\">Control</div>
            <div id=\"tab-config\" class=\"tab\" onclick=\"handleClickTab('config')\">Configuration</div>
        </div>
        <div id=\"control\" class=\"content visible\">
            <div class=\"grid\">
                <span>Status</span>
                <span id=\"control_state\">{}</span>
                <span>Voltage</span>
                <span id=\"voltage_gleeble\">{}</span>
                <span>Position Parallel</span>
                <span id=\"position_parallel\">{}</span>
                <span>Busy Parallel</span>
                <span id=\"busy_parallel\">{}</span>
                <span>Position Cross</span>
                <span id=\"position_cross\">{}</span>
                <span>Busy Cross</span>
                <span id=\"busy_cross\">{}</span>
                <span>Error</span>
                <span id=\"error\">{}</span>
                <span>Last Change</span>
                <span id=\"timestamp\">{}</span>
            </div>
            <button id=\"btn-refresh\" onclick=\"handleClickRefresh()\">Refresh</button>
        </div>
        <div id=\"config\" class=\"content\">
            <form action=\"/config\" method=\"post\">
                <div class=\"grid\">
                    <label>Serial Port</label>
                    <input name=\"serial-port\" value=\"{}\"/>
                    <label>Refresh Rate</label>
                    <input name=\"refresh-rate\" value=\"{}\"/>
                    <label>Min. Voltage</label>
                    <input name=\"volt-min\" value=\"{}\"/>
                    <label>Max. Voltage</label>
                    <input name=\"volt-max\" value=\"{}\"/>
                    <label>Restart Timeout</label>
                    <input name=\"restart-timeout\" value=\"{}\"/>
                </div>
                <button type=\"submit\">Save</button>
            </form>
        </div>
    </div>
    <script>
        function handleClickTab(type) {{
            document.getElementsByClassName('tab active')[0].classList.remove('active');
            document.querySelector('#tab-' + type).classList.add('active');

            document.querySelector('.visible').classList.remove('visible');
            document.querySelector('#' + type).classList.add('visible');
        }}

        function handleClickRefresh() {{
            fetch('/refresh')
                .then(x => x.json())
                .then(x => Object.entries(x)
                    .forEach(function([key, val]) {{
                        console.log(key, val);
                        document.querySelector('#' + key).innerHTML = val;
                    }})
                );
        }}
    </script>
</body>
    ",
        state.control_state,
        state.voltage_gleeble,
        state.position_parallel, 
        state.busy_parallel, 
        state.position_cross, 
        state.busy_cross,
        state.error.unwrap_or("".to_string()),
        state.timestamp,
        config.serial_device,
        config.cycle_time.as_millis(),
        config.voltage_min,
        config.voltage_max,
        config.restart_timeout.as_millis(),
    ))
}

async fn handle_refresh(State(state): State<AppState>) -> Json<SharedState> {
    return Json(state.0.read().unwrap().clone());
}

async fn handle_config(State(state): State<AppState>) {
    // TODO(marco): Modify shared config
    state.2.send(()).unwrap();
}

pub fn run_web_server(zaber_state: StateChannel, tx_start: Sender<()>, tx_stop: Sender<()>, config: Config) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let shared_state = (zaber_state, tx_start, tx_stop, config);
    let app: Router<_> = Router::new()
        .route("/", get(handle_default)).with_state(shared_state.clone())
        .route("/refresh", get(handle_refresh)).with_state(shared_state.clone())
        .route("/config", post(handle_config)).with_state(shared_state);

    let _ = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
