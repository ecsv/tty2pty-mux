/* SPDX-License-Identifier: MIT */

import './style.css'
import { createTerminal } from './terminal.js'
import { TelnetParser } from './telnet.js'

/* DOM refs */
const elUrl = document.getElementById('input-url')
const elBtn = document.getElementById('btn-connect')
const elDot = document.getElementById('status-dot')
const elLabel = document.getElementById('status-label')

/* Terminal */
const { term, fitAddon } = createTerminal(document.getElementById('terminal'))

term.writeln('\x1b[2m WebSocket Telnet Terminal ready.\x1b[0m')
term.writeln(
  '\x1b[2m Enter a WebSocket URL above and click CONNECT.\x1b[0m\r\n',
)

/* State */
let ws = null
let parser = null

/* Helpers */
function setStatus(state, label) {
  elDot.className = state
  elLabel.textContent = label
}

window.addEventListener('resize', () => {
  fitAddon.fit()
})

/* Connect / Disconnect */
function connect() {
  const url = elUrl.value.trim()
  if (!url) {
    elUrl.focus()
    return
  }

  setStatus('connecting', 'connecting...')
  elBtn.textContent = 'CANCEL'
  elBtn.className = 'disconnect'
  elUrl.disabled = true

  ws = new WebSocket(url)
  ws.binaryType = 'arraybuffer'

  parser = new TelnetParser((bytes) => {
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(bytes)
  })

  ws.onopen = () => {
    setStatus('connected', 'connected')
    elBtn.textContent = 'DISCONNECT'
    term.writeln('\r\n\x1b[32m-- connected --\x1b[0m\r\n')
    parser.reset()
    term.focus()
  }

  ws.onmessage = (ev) => {
    const data =
      ev.data instanceof ArrayBuffer
        ? new Uint8Array(ev.data)
        : new TextEncoder().encode(ev.data)

    const clean = parser.process(data)
    if (clean) term.write(clean)
  }

  ws.onerror = () => {
    setStatus('error', 'error')
    term.writeln('\r\n\x1b[31m-- connection error --\x1b[0m\r\n')
  }

  ws.onclose = (ev) => {
    ws = null
    parser = null
    setStatus('', 'disconnected')
    elBtn.textContent = 'CONNECT'
    elBtn.className = ''
    elUrl.disabled = false
    term.writeln(`\r\n\x1b[33m-- disconnected (${ev.code}) --\x1b[0m\r\n`)
  }
}

function disconnect() {
  if (ws) ws.close(1000, ' ')
}

/* Button & keyboard */
elBtn.addEventListener('click', () => {
  if (ws) disconnect()
  else connect()
})
elUrl.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') connect()
})

/* Send key input to server */
term.onData((data) => {
  if (!ws || ws.readyState !== WebSocket.OPEN) return
  ws.send(new TextEncoder().encode(data))
})

/* Auto-connect from URL query param */
;(function autoConnect() {
  const params = new URLSearchParams(window.location.search)
  let target = params.get('url') || params.get('ws') || params.get('host')

  if (!target) {
    const firstKey = [...params.keys()][0] || ''
    if (firstKey.startsWith('ws')) target = firstKey
  }

  if (target) {
    elUrl.value = /^wss?:\/\//.test(target) ? target : 'ws://' + target
    connect()
  }
})()
