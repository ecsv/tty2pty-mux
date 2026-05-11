/* SPDX-License-Identifier: MIT */

import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'

export function createTerminal(mountEl) {
  const term = new Terminal({
    theme: {
      background: '#000000',
      foreground: '#b2b2b2',
      selectionBackground: 'rgba(57,255,20,0.25)',
      black: '#000000',
      brightBlack: '#686868',
      red: '#b21818',
      brightRed: '#ff5454',
      green: '#18b218',
      brightGreen: '#54ff54',
      yellow: '#b26818',
      brightYellow: '#ffff54',
      blue: '#1818b2',
      brightBlue: '#5454ff',
      magenta: '#b218b2',
      brightMagenta: '#ff54ff',
      cyan: '#18b2b2',
      brightCyan: '#54ffff',
      white: '#b2b2b2',
      brightWhite: '#55ffff',
    }
  })

  const fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.open(mountEl)
  fitAddon.fit()
  term.focus()

  return { term, fitAddon }
}
