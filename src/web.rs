use std::sync::{Arc, Condvar, Mutex, RwLock};

use axum::{extract::State, response::Html, routing::get, Router};

use crate::control::{SharedState, StateChannel, StopChannel};
type AppState = (Arc<RwLock<SharedState>>, Arc<(Mutex<bool>, Condvar)>);

async fn page(State(state): State<AppState>) -> Html<String> {
    let shared_state = state.0;
    let shared_state = shared_state.read().unwrap();
    let state = shared_state.clone();
    drop(shared_state);

    Html(format!(
        "
<head>
    <style>
        .grid {{
            display: grid;
            grid-template-columns: max-content 1fr;
            gap: 5px 20px
        }}
    </style>
</head>
<body>
    <h1>State</h1>
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
    <h1>Configuration</h1>
    <form class=\"grid\">
        <label>Refresh Rate</label>
        <input name=\"refresh-rate\" value=\"\"/>
        <label>Min. Voltage</label>
        <input name=\"volt-min\" value=\"\"/>
        <label>Max. Voltage</label>
        <input name=\"volt-max\" value=\"\"/>
    </form>
    <script>
        console.log('hello');
    </script>
</body>
    ",
        state.position_parallel, state.busy_parallel, state.position_cross, state.busy_cross
    ))
}

pub fn run_web_server(zaber_state: StateChannel, stop: StopChannel) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let shared_state = (zaber_state, stop);
    let app: Router<_> = Router::new().route("/", get(page)).with_state(shared_state);

    let _ = rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
