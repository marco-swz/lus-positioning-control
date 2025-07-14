const MICROSTEP_SIZE = 0.49609375; //µm
const MAX_POS = 201574; // microsteps
var globals = {
    /** @type {?WebSocket} */
    socket: null,
    /** @type {'Tracking' | 'Manual'} */
    controlMode: 'Tracking',
    /** @type {?string} */
    errorMessage: null,
    stopTriggered: false,
};


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
    data['control_mode'] = globals.controlMode;
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
    document.querySelector('#inp-pos-target-coax').value = steps2mm(document.querySelector('#inp-pos-coax').value);
    document.querySelector('#inp-pos-target-cross').value = steps2mm(document.querySelector('#inp-pos-cross').value);
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
    console.assert(globals.socket != null, 'Websocket not initialized');
    const posCoax = document.querySelector('#inp-pos-coax').value
    const posCross = document.querySelector('#inp-pos-cross').value
    globals.socket.send(posCoax + ' ' + posCross);
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

            globals.controlMode = document.querySelector('select[name="control_mode"]').value;
            document.querySelector('#btn-change-mode').style.visibility = 'hidden';
            connectWebsocket();
        });
}

function handleChangeMode() {
    if (globals.controlMode !== this.value) {
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

function connectWebsocket() {
    globals.socket = new WebSocket(`ws://${IP_ADDR}:${PORT}/ws`);
    let $btnStart = document.querySelector('#btn-start');
    let $btnStop = document.querySelector('#btn-stop');

    globals.socket.addEventListener("message", (event) => {
        const data = JSON.parse(event.data);
        const state = data['control_state'];
        document.querySelector('#control_state').value = state;

        if (globals.errorMessage != null) {
            globals.errorMessage = null;
            document.querySelector('#btn-show-error').style.visibility = 'hidden';
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
        switch (state) {
            case 'Running':
                $btnStart.hidden = true;
                $btnStop.hidden = false;
                document.querySelector('#inp-voltage1').value = data['voltage'][0];
                document.querySelector('#inp-voltage2').value = data['voltage'][1];
                document.querySelector('#inp-pos-actual-coax').value = steps2mm(data['position'][0]);
                document.querySelector('#inp-pos-actual-cross').value = steps2mm(data['position'][1]);
        
                if (globals.controlMode === 'Tracking') {
                    document.querySelector('#inp-pos-coax').disabled = true;
                    document.querySelector('#inp-pos-cross').disabled = true;
                    document.querySelector('#inp-pos-target-coax').disabled = true;
                    document.querySelector('#inp-pos-target-cross').disabled = true;
                    document.querySelector('#inp-pos-target-coax').value = steps2mm(data['target'][0]);
                    document.querySelector('#inp-pos-target-cross').value = steps2mm(data['target'][1]);
                } else {
                    document.querySelector('#inp-pos-coax').disabled = false;
                    document.querySelector('#inp-pos-cross').disabled = false;
                    document.querySelector('#inp-pos-target-coax').disabled = false;
                    document.querySelector('#inp-pos-target-cross').disabled = false;
                }
                break;
            case 'Error':
                if(globals.errorMessage !== data['error']) {
                    globals.errorMessage = data['error'];
                    document.querySelector('#btn-show-error').style.visibility = 'visible';
                    alert(globals.errorMessage);
                }
            default:
                $btnStart.hidden = false;
                $btnStop.hidden = true;
                initInputs('Stopped');
        }
    });

    globals.socket.addEventListener('open', () => {
        document.querySelector('#ui-status').setAttribute('value', 'connected');
        document.querySelector('#ui-status').value = 'connected';
    });

    globals.socket.addEventListener('close', () => {
        document.querySelector('#btn-stop').hidden = true;
        document.querySelector('#btn-start').hidden = false;

        document.querySelector('#ui-status').setAttribute('value', 'disconnected');
        document.querySelector('#ui-status').value = 'disconnected';
        alert('The connection to the control server got lost! Check if the server is running and refresh the page.');

        globals.socket = null;
        initInputs('Stopped');
    });
}

/**
 * @param {'Stopped'|'Error'} 
 * @returns {void}
 */
function initInputs(state) {
    document.querySelector('#control_state').value = state;
    document.querySelector('#inp-pos-target-coax').value = '-';
    document.querySelector('#inp-pos-target-cross').value = '-';
    document.querySelector('#inp-pos-actual-coax').value = '-';
    document.querySelector('#inp-pos-actual-cross').value = '-';
    document.querySelector('#inp-voltage1').value = '-';
    document.querySelector('#inp-voltage2').value = '-';
    document.querySelector('#inp-pos-coax').disabled = true;
    document.querySelector('#inp-pos-cross').disabled = true;
    document.querySelector('#inp-pos-target-coax').disabled = true;
    document.querySelector('#inp-pos-target-cross').disabled = true;
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

    initInputs('Stopped');
    loadConfig();
});
