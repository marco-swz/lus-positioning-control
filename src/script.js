const MICROSTEP_SIZE = 0.49609375; //µm
const MAX_POS = 201574; // microsteps
/** @type {?WebSocket} */
var gSocket = null;
/** @type {'Tracking' | 'Manual'} */
var gControlMode = 'Tracking';
/** @type {string} */
var gErrorMessage = '';


function handleClickTab(type) {
    document.getElementsByClassName('tab active')[0].classList.remove('active');
    document.querySelector('#tab-' + type).classList.add('active');

    document.querySelector('.visible').classList.remove('visible');
    document.querySelector('#' + type).classList.add('visible');
}


function handleClickSaveConfig() {
    /** @type {HTMLFormElement} */
    const $form = document.querySelector('#form-config');
    if (!$form.checkValidity()) {
        return;
    }

    let data = Object.fromEntries(new FormData($form));
    for (let [key, val] of Object.entries(data)) {
        if (['limit_max_coax', 'limit_min_coax', 'limit_min_cross', 'limit_max_cross'].includes(key)) {
            val = mm2steps(val);
        }
        data[key] = val;
    }
    data['control_mode'] = gControlMode;
    data['mock_zaber'] = false;

    fetch('/config', {
        method: 'POST',
        body: new URLSearchParams(data),
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
    });
}

function handleClickStop() {
    fetch('/stop', {
        method: 'POST',
    });
}

function handleMousedownSliderPos(slider) {
    document.querySelector(`#inp-pos-target-${slider}`).classList.add('working');
}

function handleMouseupSliderPos(slider) {
    document.querySelector(`#inp-pos-target-${slider}`).classList.remove('working');
    sendTargetPosition();
}

function handleChangeTarget(axis) {
    let $inp = document.querySelector(`#inp-pos-${axis}`);
    $inp.value = mm2steps(this.value);
    this.value = steps2mm($inp.value);
    sendTargetPosition();
}

function sendTargetPosition() {
    console.assert(gSocket != null, 'Websocket not initialized');
    const posCoax = document.querySelector('#inp-pos-coax').value
    const posCross = document.querySelector('#inp-pos-cross').value
    gSocket.send(posCoax + ' ' + posCross);
}

function loadConfig() {
    fetch('/config')
        .then(x => x.json())
        .then(x => {
            const state = x['control_state'];
            document.querySelector('#control_state').value = state;

            document.querySelector('#inp-pos-min-coax').value = steps2mm(x['limit_min_coax']);
            document.querySelector('#inp-pos-max-coax').value = steps2mm(x['limit_max_coax']);
            document.querySelector('#inp-pos-coax').min = x['limit_min_coax'];
            document.querySelector('#inp-pos-coax').max = x['limit_max_coax'];

            document.querySelector('#inp-pos-min-cross').value = steps2mm(x['limit_min_cross']);
            document.querySelector('#inp-pos-max-cross').value = steps2mm(x['limit_max_cross']);
            document.querySelector('#inp-pos-cross').min = x['limit_min_cross'];
            document.querySelector('#inp-pos-cross').max = x['limit_max_cross'];

            for (let [key, val] of Object.entries(x)) {
                if (['limit_max_coax', 'limit_min_coax', 'limit_min_cross', 'limit_max_cross'].includes(key)) {
                    val = steps2mm(val);
                }

                const $inp = document.querySelector(`[name="${key}"]`);
                if ($inp != null) {
                    $inp.value = val;
                }
            }

            gControlMode = document.querySelector('select[name="control_mode"]').value;
            document.querySelector('#btn-change-mode').style.visibility = 'hidden';
        });
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

function handleChangeMode() {
    if (gControlMode !== this.value) {
        document.querySelector('#btn-change-mode').style.visibility = null;
    } else {
        document.querySelector('#btn-change-mode').style.visibility = 'hidden';
    }
}

function handleClickChangeMode() {
    const mode = document.querySelector('[name=control_mode]').value;
    fetch('/mode/' + mode, {
        method: 'POST',
    });

    loadConfig();
}

function connectWebsocketManual() {
    gSocket = new WebSocket('ws://localhost:8080/ws');
    let $btnStart = document.querySelector('#btn-start');
    let $btnStop = document.querySelector('#btn-stop');

    gSocket.addEventListener("message", (event) => {
        const data = JSON.parse(event.data);
        const state = data['control_state'];

        document.querySelector('#inp-voltage').value = data['voltage'];
        document.querySelector('#inp-pos-actual-coax').value = steps2mm(data['position_coax']);
        document.querySelector('#inp-pos-actual-cross').value = steps2mm(data['position_cross']);
        if (state !== "Running" || gControlMode === 'Tracking') {
            document.querySelector('#inp-pos-coax').value = data['position_coax'];
        }
        if (state !== "Running") {
            document.querySelector('#inp-pos-cross').value = data['position_cross'];
        }
        
        if (gControlMode === 'Tracking') {
            document.querySelector('#inp-pos-target-coax').value = steps2mm(data['target_coax']);
        }

        gErrorMessage = data['error'];

        if (state === 'Error') {
            document.querySelector('#btn-show-error').style.visibility = 'visible';
        } else {
            document.querySelector('#btn-show-error').style.visibility = 'hidden';
        }

        document.querySelector('#control_state').value = state;
        if (state !== 'Stopped') {
            $btnStart.hidden = true;
            $btnStop.hidden = false;
        } else {
            $btnStart.hidden = false;
            $btnStop.hidden = true;
        }

        if (data['busy_coax']) {
            document.querySelector('#inp-pos-actual-coax').classList.add('working');
        } else {
            document.querySelector('#inp-pos-actual-coax').classList.remove('working');
        }
        if (data['busy_cross']) {
            document.querySelector('#inp-pos-actual-cross').classList.add('working');
        } else {
            document.querySelector('#inp-pos-actual-cross').classList.remove('working');
        }

        if (state === 'Running') {
            if (gControlMode === 'Tracking') {
                document.querySelector('#inp-pos-coax').disabled = true;
                document.querySelector('#inp-pos-min-coax').disabled = true;
                document.querySelector('#inp-pos-max-coax').disabled = true;
                document.querySelector('#inp-pos-target-cross').disabled = true;
            } else {
                document.querySelector('#inp-pos-coax').disabled = false;
                document.querySelector('#inp-pos-min-coax').disabled = false;
                document.querySelector('#inp-pos-max-coax').disabled = false;
                document.querySelector('#inp-pos-target-coax').disabled = false;
            }
            document.querySelector('#inp-pos-cross').disabled = false;
            document.querySelector('#inp-pos-min-cross').disabled = false;
            document.querySelector('#inp-pos-max-cross').disabled = false;
            document.querySelector('#inp-pos-target-cross').disabled = false;
        } else {
            document.querySelector('#inp-pos-coax').disabled = true;
            document.querySelector('#inp-pos-min-coax').disabled = true;
            document.querySelector('#inp-pos-max-coax').disabled = true;
            document.querySelector('#inp-pos-target-coax').disabled = true;
            document.querySelector('#inp-pos-cross').disabled = true;
            document.querySelector('#inp-pos-min-cross').disabled = true;
            document.querySelector('#inp-pos-max-cross').disabled = true;
            document.querySelector('#inp-pos-target-cross').disabled = true;
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

function steps2mm(steps) {
    return steps * MICROSTEP_SIZE / 1000;
}

function mm2steps(millis) {
    return Math.round(millis * 1000. / MICROSTEP_SIZE);
}

loadConfig();
loadOpcua();
connectWebsocketManual();

document.addEventListener('DOMContentLoaded', () => {
    const $inpTargetCoax = document.querySelector('#inp-pos-target-coax');
    const $sliderTargetCoax = document.querySelector('#inp-pos-coax');
    $sliderTargetCoax.addEventListener('input', (e) => {
        $inpTargetCoax.value = steps2mm(e.currentTarget.value);
    })

    const $inpTargetCross = document.querySelector('#inp-pos-target-cross');
    const $sliderTargetCross = document.querySelector('#inp-pos-cross');
    $sliderTargetCross.addEventListener('input', (e) => {
        $inpTargetCross.value = steps2mm(e.currentTarget.value);
    })
});
