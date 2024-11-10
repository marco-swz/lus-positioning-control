use std::sync::{Arc, RwLock};

use axum::{extract::State, response::Html, routing::get, Json, Router};
use crossbeam_channel::Sender;

use crate::control::{SharedState, StateChannel};
type AppState = (Arc<RwLock<SharedState>>, Sender<()>, Sender<()>);

async fn handle_default(State(state): State<AppState>) -> Html<String> {
    let shared_state = state.0;
    let shared_state = shared_state.read().unwrap();
    let state = shared_state.clone();
    drop(shared_state);

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
            width: 300px;
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
                <span>{}</span>
                <span>Position Parallel</span>
                <span>{}</span>
                <span>Busy Parallel</span>
                <span>{}</span>
                <span>Position Cross</span>
                <span>{}</span>
                <span>Busy Cross</span>
                <span>{}</span>
            </div>
            <button id=\"btn-refresh\" onclick=\"handleClickRefresh()\">Refresh</button>
        </div>
        <div id=\"config\" class=\"content\">
            <form class=\"grid\">
                <label>Refresh Rate</label>
                <input name=\"refresh-rate\" value=\"\"/>
                <label>Min. Voltage</label>
                <input name=\"volt-min\" value=\"\"/>
                <label>Max. Voltage</label>
                <input name=\"volt-max\" value=\"\"/>
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
                .then(x => console.log(x));
        }}
    </script>
</body>
    ",
        state.control_state,
        state.position_parallel, 
        state.busy_parallel, 
        state.position_cross, 
        state.busy_cross
    ))
}

async fn handle_refresh(State(state): State<AppState>) -> String {
    return "[\"hello\"]".to_string();
}

pub fn run_web_server(zaber_state: StateChannel, tx_start: Sender<()>, tx_stop: Sender<()>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let shared_state = (zaber_state, tx_start, tx_stop);
    let app: Router<_> = Router::new()
        .route("/", get(handle_default)).with_state(shared_state.clone())
        .route("/refresh", get(handle_refresh)).with_state(shared_state);

    let _ = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
