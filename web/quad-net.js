function on_init() {
}

var register_plugin = function (importObject) {
    importObject.env.ws_connect = ws_connect;
    importObject.env.ws_is_connected = ws_is_connected;
    importObject.env.ws_send = ws_send;
    importObject.env.ws_try_recv = ws_try_recv;

    importObject.env.http_make_request = http_make_request;
    importObject.env.http_try_recv = http_try_recv;
}

miniquad_add_plugin({ register_plugin, on_init, version: 1, name: "quad_net" });

var quad_socket;
var connected = 0;
var received_buffer = [];
var send_buffer = [];

function ws_is_connected() {
    return connected;
}

function ws_connect(addr) {
    var raw_addr = consume_js_object(addr);
    if (typeof location !== 'undefined' && location.host) {
        var protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
        var path = '/ws';
        if (typeof raw_addr === 'string' && raw_addr.indexOf('/') !== -1) {
            var parts = raw_addr.split('/');
            if (parts.length > 3) {
                path = '/' + parts.slice(3).join('/');
            }
        }
        raw_addr = protocol + '//' + location.host + path;
    }
    console.log('[quad-net] Connecting WebSocket to:', raw_addr);
    send_buffer = [];
    try {
        quad_socket = new WebSocket(raw_addr);
        quad_socket.binaryType = 'arraybuffer';
        quad_socket.onopen = function() {
            console.log('[quad-net] WebSocket connected successfully to', raw_addr);
            connected = 1;
            while (send_buffer.length > 0) {
                var pending = send_buffer.shift();
                try {
                    quad_socket.send(pending);
                } catch (e) {
                    console.error('[quad-net] Failed to send buffered message:', e);
                }
            }
        };
        quad_socket.onclose = function(e) {
            console.log('[quad-net] WebSocket disconnected, code:', e.code);
            connected = 0;
            send_buffer = [];
        };
        quad_socket.onerror = function(err) {
            console.error('[quad-net] WebSocket error:', err);
            connected = 0;
        };

        quad_socket.onmessage = function(msg) {
            if (typeof msg.data == "string") {
                received_buffer.push({
                    "text": 1,
                    "data": msg.data
                });
            } else {
                var text = new TextDecoder("utf-8").decode(msg.data);
                var buffer = new Uint8Array(msg.data);
                buffer.text = 1;
                buffer.data = text;
                received_buffer.push(buffer);
            }
        };
    } catch (e) {
        console.error('[quad-net] Failed to create WebSocket:', e);
        connected = 0;
    }
};

function ws_send(data) {
    var obj = consume_js_object(data);
    var payload;
    if (typeof obj === "string") {
        payload = obj;
    } else if (obj && obj.buffer !== undefined) {
        payload = obj.buffer;
    } else {
        payload = obj;
    }

    if (!quad_socket) {
        console.warn('[quad-net] ws_send called with no socket');
        return;
    }

    if (quad_socket.readyState === 1 /* WebSocket.OPEN */) {
        try {
            quad_socket.send(payload);
        } catch (e) {
            console.error('[quad-net] Error sending message on open socket:', e);
        }
    } else if (quad_socket.readyState === 0 /* WebSocket.CONNECTING */) {
        // Queue for dispatch upon onopen
        send_buffer.push(payload);
    } else {
        console.warn('[quad-net] ws_send dropped packet, socket state:', quad_socket.readyState);
    }
};

function ws_try_recv() {
    if (received_buffer.length != 0) {
        return js_object(received_buffer.shift())
    }
    return -1;
}


var uid = 0;
var ongoing_requests = {};

function http_try_recv(cid) {
    if (ongoing_requests[cid] != undefined && ongoing_requests[cid] != null) {
        var data = ongoing_requests[cid];
        ongoing_requests[cid] = null;
        return js_object(data);
    }
    return -1;
}

function http_make_request(scheme, url, body, headers) {
    var cid = uid;

    uid += 1;

    var scheme_string;
    if (scheme == 0) {
        scheme_string = 'POST';
    }
    if (scheme == 1) {
        scheme_string = 'PUT';
    }
    if (scheme == 2) {
        scheme_string = 'GET';
    }
    if (scheme == 3) {
        scheme_string = 'DELETE';
    }
    var url_string = consume_js_object(url);
    var body_string = consume_js_object(body);
    var headers_obj = consume_js_object(headers);
    var xhr = new XMLHttpRequest();
    xhr.open(scheme_string, url_string, true);
    xhr.responseType = 'arraybuffer';
    for (const header in headers_obj) {
        xhr.setRequestHeader(header, headers_obj[header]);
    }
    xhr.onload = function (e) {
        if (this.status == 200) {
            var uInt8Array = new Uint8Array(this.response);
            
            ongoing_requests[cid] = uInt8Array;
        }
    }
    xhr.onerror = function (e) {
        // todo: let rust know and put Error to ongoing requests
        console.error("Failed to make a request");
        console.error(e);
    };

    xhr.send(body_string);

    return cid;
}
