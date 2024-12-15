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
        .then(x => {
            loadConfig();
            if (x.ok) {
                alert('New config loaded');
            } else {
                alert('Error while loading new config');
            }
        })
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
    const posParallel = document.querySelector('#inp-pos-parallel').value;
    const posCross = document.querySelector('#inp-pos-cross').value;
    gSocket.send(posParallel + ' ' + posCross);
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
        .then(x => Object.entries(x)
            .forEach(function([key, val]) {
                if (typeof val === 'object') {
                    let $fieldset = document.querySelector(`fieldset#${key}`);
                    if ($fieldset == null) {
                        return;
                    }
                    for (const [k, v] of Object.entries(val)) {
                        let $inp = document.querySelector(`#${key} [name="${k}"]`);
                        if ($inp != null) {
                            $inp.value = v;
                        }
                    }
                } else {
                    document.querySelector(`[name=${key}]`).value = val;
                }
            })
        );
}

function connectWebsocketManual() {
    gSocket = new WebSocket('ws://localhost:8080/ws');
    let $btnStart = document.querySelector('#btn-start');
    let $btnStop = document.querySelector('#btn-stop');

    // Listen for messages
    gSocket.addEventListener("message", (event) => {
        const data = JSON.parse(event.data);
        Object.entries(data)
            .forEach(function([key, val]) {
                if (key === "timestamp") {
                    let [date, time] = val.split('T');
                    time = time.split('.')[0];
                    val = date + ' ' + time;
                }
                document.querySelector('#' + key).value = val;
            });
        document.querySelector('#inp-pos-actual-parallel').value = data['position_parallel'];
        document.querySelector('#inp-pos-actual-cross').value = data['position_cross'];

        if (data['control_state'] !== 'Stopped') {
            $btnStart.hidden = true;
            $btnStop.hidden = false;
        } else {
            $btnStart.hidden = false;
            $btnStop.hidden = true;
        }
    });

    gSocket.addEventListener('open', () => {
        document.querySelector('#ui-status').setAttribute('value', 'connected');
        document.querySelector('#ui-status').value = 'connected';
    });

    gSocket.addEventListener('close', () => {
        document.querySelector('#ui-status').setAttribute('value', 'disconnected');
        document.querySelector('#ui-status').value = 'disconnected';
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
