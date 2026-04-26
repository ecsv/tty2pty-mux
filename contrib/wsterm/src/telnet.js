/* SPDX-License-Identifier: MIT */

export const IAC = 255

export const WILL = 251
export const WONT = 252
export const DO = 253
export const DONT = 254

export const OPTION_BINARY = 0
export const OPTION_ECHO = 1
export const OPTION_SGA = 3

export class TelnetParser {
  constructor(sendFn) {
    this._send = sendFn
    this._inIAC = false
  }

  reset() {
    this._inIAC = false
  }

  /** Process raw bytes; returns clean bytes to pass to the terminal. */
  process(buf) {
    const out = []

    for (let i = 0; i < buf.length; i++) {
      const b = buf[i]

      if (this._inIAC) {
        this._inIAC = false
        if (b === IAC) {
          out.push(0xff)
        } else if (b === WILL || b === WONT || b === DO || b === DONT) {
          if (i + 1 < buf.length) {
            this._handleOption(b, buf[++i])
          }
        }
        continue
      }

      if (b === IAC) {
        this._inIAC = true
        continue
      }
      out.push(b)
    }

    return out.length > 0 ? new Uint8Array(out) : null
  }

  _respond(cmd, opt) {
    this._send(new Uint8Array([IAC, cmd, opt]))
  }

  _handleOption(cmd, opt) {
    if (cmd === DO || cmd === DONT) {
      if (opt === OPTION_SGA || opt === OPTION_BINARY) {
        this._respond(WILL, opt)
      } else {
        this._respond(WONT, opt)
      }
    } else if (cmd === WILL || cmd === WONT) {
      if (opt === OPTION_ECHO || opt === OPTION_SGA || opt === OPTION_BINARY) {
        this._respond(DO, opt)
      } else {
        this._respond(DONT, opt)
      }
    }
  }
}
