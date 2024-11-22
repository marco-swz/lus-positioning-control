/** @type {?WebSocket} */
var gSocket = null;

function handleClickTab(type) {
    document.getElementsByClassName('tab active')[0].classList.remove('active');
    document.querySelector('#tab-' + type).classList.add('active');

    document.querySelector('.visible').classList.remove('visible');
    document.querySelector('#' + type).classList.add('visible');
}

function handleClickRefresh() {
    fetch('/refresh')
        .then(x => x.json())
        .then(x => Object.entries(x)
            .forEach(function([key, val]) {
                if (key === "timestamp") {
                    let [date, time] = val.split('T');
                    time = time.split('.')[0];
                    val = date + ' ' + time;
                }
                document.querySelector('#' + key).value = val;
            })
        );
}

function handleClickSaveConfig() {
    /** @type {HTMLFormElement} */
    const $form = document.querySelector('#form-config');
    if (!$form.checkValidity()) {
        return;
    }

    fetch('/config', {
        method: 'POST',
        body: new URLSearchParams(new FormData($form)),
        headers: {
            "Content-Type": "application/x-www-form-urlencoded",
        },
    })
        .then(() => loadConfig());
}

function handleClickStart() {
    fetch('/start', {
        method: 'POST',
    })
        .then(() => handleClickRefresh());
}

function handleClickStop() {
    fetch('/stop', {
        method: 'POST',
    })
        .then(() => handleClickRefresh());
}

function handleMouseupSliderPos() {
    console.assert(gSocket != null, 'Websocket not initialized');
    console.log(this.value);
    gSocket.send(this.value);
}

function loadConfig() {
    fetch('/config')
        .then(x => x.json())
        .then(x => Object.entries(x)
            .forEach(function([key, val]) {
                document.querySelector(`[name=${key}]`).value = val;
            })
        );
}

function loadOpcua() {
    fetch('/opcua')
        .then(x => x.json())
        .then(x => {
            let $form = document.querySelector('#form-opcua > div');
            Object.entries(x)
                .forEach(function([key, val]) {
                    let $label = document.createElement('label');
                    $label.innerHTML = key;

                    let $input = document.createElement('input');
                    $input.name = key;
                    $input.value = val;

                    $form.append($label);
                    $form.append($input);
                })
        });
}

function connectWebsocketManual() {
    gSocket = new WebSocket('ws://localhost:8080/ws');

    // Listen for messages
    gSocket.addEventListener("message", (event) => {
        console.log("Message from server ", event.data);
    });

}

handleClickRefresh();
loadConfig();
loadOpcua();
connectWebsocketManual();

document.addEventListener('DOMContentLoaded', () => {
    const $inpTarget = document.querySelector('#inp-pos-target');
    document.querySelector('#inp-pos').addEventListener('input', (e) => {
        $inpTarget.value = e.currentTarget.value;
    })

});
