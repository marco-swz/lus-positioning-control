const MICROSTEP_SIZE = 0.49609375; //µm
const MAX_POS = 201574; // microsteps
const globals = {
    /** @type {?WebSocket} */
    socket: null,
    /** @type {'Tracking' | 'Manual'} */
    controlMode: 'Tracking',
    /** @type {?string} */
    errorMessage: null,
    stopTriggered: false,
    /** @type {Array<HTMLInputElement>} */
    $$targets: null,
    /** @type {Array<HTMLInputElement>} */
    $$positions: null,
    /** @type {Array<HTMLInputElement>} */
    $$sliders: null,
    /** @type {Array<HTMLInputElement>} */
    $$voltages: null,
    /** @type {HTMLButtonElement} */
    $btnStart: null,
    /** @type {HTMLButtonElement} */
    $btnStop: null,
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
    globals.$$targets
        .forEach(($target, i) => $target.value = steps2mm(globals.$$sliders[i].value));

    fetch('/start', {
        method: 'POST',
    }).then(() => {
        resetError();
    });
}

function resetError() {
    globals.errorMessage = null;
    document.querySelector('#btn-show-error').style.visibility = 'hidden';
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
    const posCoax = globals.$$sliders[0].value;
    const posCross = globals.$$sliders[1].value;
    globals.socket.send(posCoax + ' ' + posCross);
}

function loadConfig() {
    fetch('/config')
        .then(x => x.json())
        .then(x => {
            document.querySelector('#inp-pos-min-coax').value = steps2mm(x['limit_min_coax']);
            document.querySelector('#inp-pos-max-coax').value = steps2mm(x['limit_max_coax']);
            globals.$$sliders[0].min = x['limit_min_coax'];
            globals.$$sliders[0].max = x['limit_max_coax'];

            document.querySelector('#inp-pos-min-cross').value = steps2mm(x['limit_min_cross']);
            document.querySelector('#inp-pos-max-cross').value = steps2mm(x['limit_max_cross']);
            globals.$$sliders[1].min = x['limit_min_cross'];
            globals.$$sliders[1].max = x['limit_max_cross'];

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

    if (globals.errorMessage != null) {
        resetError();
    }

    globals.$$targets.forEach(($target, i) => $target.value = steps2mm(globals.$$sliders[i].value))

    globals.socket.addEventListener("message", (event) => {
        const data = JSON.parse(event.data);
        const state = data['control_state'];
        document.querySelector('#control_state').value = state;

        for (let i = 0; i < 2; ++i) {
            if (data['is_busy'][i]) {
                globals.$$positions[i].classList.add('working');
            } else {
                globals.$$positions[i].classList.remove('working');
            }
        }

        switch (state) {
            case 'Running':
                globals.$btnStart.hidden = true;
                globals.$btnStop.hidden = false;
                for (let i = 0; i < 2; ++i) {
                    globals.$$positions[i].value = steps2mm(data['position'][i]);
                    globals.$$voltages[i].value = data['voltage'][i];

                    if (globals.controlMode === 'Tracking') {
                        globals.$$sliders[i].disabled = true;
                        globals.$$targets[i].disabled = true;
                        globals.$$targets[i].value = steps2mm(data['target'][i]);
                    } else {
                        globals.$$sliders[i].disabled = false;
                        globals.$$targets[i].disabled = false;
                    }
                }
                break;
            case 'Error':
                if (globals.errorMessage !== data['error']) {
                    globals.errorMessage = data['error'];
                    document.querySelector('#btn-show-error').style.visibility = 'visible';
                    alert(globals.errorMessage);
                }
            default:
                globals.$btnStart.hidden = false;
                globals.$btnStop.hidden = true;
                initInputs('Stopped');
        }
    });

    globals.socket.addEventListener('open', () => {
        document.querySelector('#ui-status').setAttribute('value', 'connected');
        document.querySelector('#ui-status').value = 'connected';
    });

    globals.socket.addEventListener('close', () => {
        globals.$btnStop.hidden = true;
        globals.$btnStart.hidden = false;

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

    for (let i = 0; i < 2; ++i) {
        globals.$$targets[i].value = '-';
        globals.$$positions[i].value = '-';
        globals.$$voltages[i].value = '-';
        globals.$$sliders[i].disabled = true;
        globals.$$targets[i].disabled = true;
    }
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
    globals.$btnStart = document.querySelector('#btn-start');
    globals.$btnStop = document.querySelector('#btn-stop');
    globals.$$sliders = [
        document.querySelector('#inp-pos-coax'),
        document.querySelector('#inp-pos-cross')
    ];
    globals.$$targets = [
        document.querySelector('#inp-pos-target-coax'),
        document.querySelector('#inp-pos-target-cross'),
    ];
    globals.$$voltages = [
        document.querySelector('#inp-voltage1'),
        document.querySelector('#inp-voltage2'),
    ];
    globals.$$positions = [
        document.querySelector('#inp-pos-actual-coax'),
        document.querySelector('#inp-pos-actual-cross'),
    ];
    for (let i = 0; i < 2; ++i) {
        globals.$$sliders[i].addEventListener('input', (e) => {
            globals.$$targets[i].value = steps2mm(e.currentTarget.value);
        })
    }

    initInputs('Stopped');
    loadConfig();
});
