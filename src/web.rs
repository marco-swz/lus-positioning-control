use std::sync::{Arc, RwLock};

use axum::{extract::State, response::Html, routing::get, Json, Router};
use crossbeam_channel::Sender;

use crate::control::{SharedState, StateChannel};
type AppState = (Arc<RwLock<SharedState>>, Sender<()>, Sender<()>);

async fn handleDefault(State(state): State<AppState>) -> Html<String> {
    let shared_state = state.0;
    let shared_state = shared_state.read().unwrap();
    let state = shared_state.clone();
    drop(shared_state);

    Html(format!(
        "
<head>
    <style>
        .content {{
            width: 300px;
            display: none;
            border: 1px solid grey;
            padding: 10px;
        }}

        .grid {{
            grid-template-columns: max-content 1fr;
            gap: 5px 20px;
        }}

        .visible {{
            display: grid;
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
        }}

        .tab.active {{
            border-bottom-color: transparent;
            color: black;
            background-color: white;
        }}
    </style>
</head>
<body>
    <div id=\"tabs\">
        <div id=\"tab-control\" class=\"tab active\" onclick=\"handleClickTab('control')\">Control</div>
        <div id=\"tab-config\" class=\"tab\" onclick=\"handleClickTab('config')\">Configuration</div>
    </div>
    <div id=\"control\" class=\"content visible\">
        <div class=\"grid\">
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
    <form id=\"config\" class=\"content grid\">
        <label>Refresh Rate</label>
        <input name=\"refresh-rate\" value=\"\"/>
        <label>Min. Voltage</label>
        <input name=\"volt-min\" value=\"\"/>
        <label>Max. Voltage</label>
        <input name=\"volt-max\" value=\"\"/>
    </form>
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
        state.position_parallel, state.busy_parallel, state.position_cross, state.busy_cross
    ))
}

async fn handleRefresh(State(state): State<AppState>) -> String {
    return "[\"hello\"]".to_string();
}

pub fn run_web_server(zaber_state: StateChannel, tx_start: Sender<()>, tx_stop: Sender<()>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let shared_state = (zaber_state, tx_start, tx_stop);
    let app: Router<_> = Router::new()
        .route("/", get(handleDefault)).with_state(shared_state.clone())
        .route("/refresh", get(handleRefresh)).with_state(shared_state);

    let _ = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
