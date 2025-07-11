const MICROSTEP_SIZE = 0.49609375; //µm
const MAX_POS = 201574; // microsteps
/** @type {?WebSocket} */
var gSocket = null;
/** @type {'Tracking' | 'Manual'} */
var gControlMode = 'Tracking';
/** @type {?string} */
var gErrorMessage = null;


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

        if (['accel_cross', 'accel_coax'].includes(key)) {
            val = accel2steps(val);
        }

        if (['maxspeed_cross', 'maxspeed_coax'].includes(key)) {
            val = vel2steps(val);
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
            if (x.ok) {
                alert('New config loaded');
                loadConfig();
                return;
            }

            return x.text();
        })
        .then(x => {
            if (!x.includes(':')) {
                alert('Error while loading new config:\n' + x);
                return;
            }

            const [name, msg] = x.split(':');
            let $inp = document.querySelector(`[name=${name}]`);
            $inp.classList.add('invalid');
            $inp.onchange = () => {
                $inp.classList.remove('invalid');
                $inp.onchange = null;
            };
            alert('Error while loading new config:\n' + msg.trim());
        });
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

function handleClickRegisterAdc(idx) {
    fetch('/adc/' + idx, { method: 'post' })
        .then(x => x.json()) // TODO(marco): Create error dialog
        .then(x => console.log(x));
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

                if (['accel_cross', 'accel_coax'].includes(key)) {
                    val = steps2accel(val);
                }

                if (['maxspeed_cross', 'maxspeed_coax'].includes(key)) {
                    val = steps2vel(val);
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
    gSocket = new WebSocket(`ws://${IP_ADDR}:${PORT}/ws`);
    let $btnStart = document.querySelector('#btn-start');
    let $btnStop = document.querySelector('#btn-stop');

    gSocket.addEventListener("message", (event) => {
        const data = JSON.parse(event.data);
        const state = data['control_state'];

        document.querySelector('#inp-voltage1').value = data['voltage'][0];
        document.querySelector('#inp-voltage2').value = data['voltage'][1];
        document.querySelector('#inp-pos-actual-coax').value = steps2mm(data['position_coax']);
        document.querySelector('#inp-pos-actual-cross').value = steps2mm(data['position_cross']);
        if (state !== "Running" || gControlMode === 'Tracking') {
            document.querySelector('#inp-pos-coax').value = data['position_coax'];
            document.querySelector('#inp-pos-cross').value = data['position_cross'];
        }
        
        if (gControlMode === 'Tracking') {
            document.querySelector('#inp-pos-target-coax').value = steps2mm(data['target_coax']);
            document.querySelector('#inp-pos-target-cross').value = steps2mm(data['target_cross']);
        }


        if (state === 'Error') {
            if(gErrorMessage !== data['error']) {
                gErrorMessage = data['error'];
                document.querySelector('#btn-show-error').style.visibility = 'visible';
                alert(gErrorMessage);
            }
        } else {
            if (gErrorMessage != null) {
                gErrorMessage = null;
                document.querySelector('#btn-show-error').style.visibility = 'hidden';
            }
        }

        document.querySelector('#control_state').value = state;
        if (state !== 'Running') {
            $btnStart.hidden = false;
            $btnStop.hidden = true;
        } else {
            $btnStart.hidden = true;
            $btnStop.hidden = false;
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
                document.querySelector('#inp-pos-cross').disabled = true;
                document.querySelector('#inp-pos-target-coax').disabled = true;
                document.querySelector('#inp-pos-target-cross').disabled = true;
            } else {
                document.querySelector('#inp-pos-coax').disabled = false;
                document.querySelector('#inp-pos-cross').disabled = false;
                document.querySelector('#inp-pos-target-coax').disabled = false;
                document.querySelector('#inp-pos-target-cross').disabled = false;
            }
        } else {
            document.querySelector('#inp-pos-coax').disabled = true;
            document.querySelector('#inp-pos-cross').disabled = true;
            document.querySelector('#inp-pos-target-coax').disabled = true;
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
        alert('The connection to the control server got lost! Check if the server is running and refresh the page.');
    });
}

function steps2mm(steps) {
    return steps * MICROSTEP_SIZE / 1000;
}

function mm2steps(millis) {
    return Math.round(millis * 1000. / MICROSTEP_SIZE);
}

function steps2vel(steps) {
    return steps * MICROSTEP_SIZE / 1.6384 / 1000;
}

function vel2steps(accel) {
    return Math.round(accel * 1000 * 1.6384 / MICROSTEP_SIZE);
}

function steps2accel(steps) {
    return steps * MICROSTEP_SIZE * 10 / 1.6384;
}

function accel2steps(accel) {
    return Math.round(accel * 1.6384 / MICROSTEP_SIZE / 10);
}

loadConfig();
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
