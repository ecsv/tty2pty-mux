/* SPDX-License-Identifier: MIT */

import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'

export function createTerminal(mountEl) {
  const term = new Terminal({})

  const fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.open(mountEl)
  fitAddon.fit()
  term.focus()

  return { term, fitAddon }
}
