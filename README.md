<!-- SPDX-License-Identifier: GPL-2.0 -->

TTY2PTY mux
===========

Motivation
----------

When you work on a serial console, there can only be one writer/reader. Either
because it is exclusively used via `TIOCEXCL` (like with `screen`) or because
multiple readers consume bytes from each other.

To work around this, a simple multiplexer binary can be used. It just needs to
be started against a `tty` device and then multiple `pty` devices can be
created.


Usage
-----

```
cargo deb
sudo apt install ./target/debian/tty2pty-mux_0.1.0-1_amd64.deb
tty2pty-mux --device /dev/ttyUSB0 -b 115200 --pty /tmp/asd --pty /tmp/foobar
```

Programs like `minicom` or `picocom` can then be connected to the `pty`
devices:


```
picocom /tmp/foobar

# not recommended because it doesn't lock the serial and thus causes problems
# with other programs which might have the pty open
minicom -D /tmp/asd
```

Please don't connect with `screen` because it opens the `pty` using `TIOCEXCL`.
After you close the `pty` in screen, no one else will ever be able to connect
to the `pty` again.



Autostart
---------

It is possible to automatically start the muxer when the correct `tty` device
is attached.


The serial of the tty needs to be found:

```
udevadm info -q all -n /dev/ttyUSB0
```

This can then be added to a udev rule `/etc/udev/rules.d/99-tty-mux-autostart.rules`

```
ACTION=="add", SUBSYSTEM=="tty", ENV{ID_SERIAL}=="sven_FT230XS_USB2UART_D38FDRJJ", TAG+="systemd", ENV{SYSTEMD_WANTS}="tty_sven_muxer@%k.service"
```

The service can then be created in `/etc/systemd/system/tty_sven_muxer@.service`

```
[Unit]
Description=Run tty2mux on %I
After=dev-%i.device
BindsTo=dev-%i.device

[Service]
Type=simple
User=sven
Group=sven
ExecStart=/usr/bin/tty2pty-mux --device /dev/%I -b 115200 --pty /tmp/ttysven_qa --pty /tmp/ttysven0

[Install]
WantedBy=multi-user.target
```

It is important to change the path to the binary and the username and group.

After the systemd services are reloaded:

```
sudo systemctl daemon-reload
```

Unplugging and plugging in the serial adapter should create the PTY devices.



Telnet support
--------------

PTYs are easy to use with terminal emulation programs. But they need a device for
each program that wants to connect. `tty2pty-mux` is also not informed about
connected/disconnected terminal emulators. It is therefore not possible to
perform special actions based on the connected clients.

It can therefore be beneficial to connect to a TCP port via telnet to
automatically add more multiplexed clients. The support for telnet clients can
be activated using

```
tty2pty-mux --device /dev/%I -b 115200 --telnet 127.0.0.1:5550
```

A client like `inetutils-telnet` can be used to connect to it:

```
telnet 127.0.0.1 5550
```

The current implementation only expects a limited set of options (binary, echo,
SGA) and can only react to `BREAK` commands.



KGDB support
------------

While KGDB(OC) can be used over normal PTYs or telnet connections, `gdb` tries
to use `^C` (`0x03`) to switch into `KGDB` mode. This is not interpreted in any
special way by `PTY`/`Telnet` and will therefore not trigger anything. The KGDB
target would expect a `BREAK` followed by `g`. As a workaround, a manual
`echo g > /proc/sysrq-trigger` is required for these connections to actually
switch to `KGDB` mode.

The `--telnet-gdb` and `--pty-gdb` arguments can be used to automatically
convert the byte `^C` (`0x03`) to `BREAK` followed by `g` on the serial
(`TTY`) device. The `--telnet-gdb` port will also automatically send a `BREAK`
followed by `g` on the serial (`TTY`) device when a new telnet client connects
to the port. This should ensure that the `target remote ...` gdb command
can directly start the handshake with the KGDB gdbstub.

This can then be used via:

```
tty2pty-mux --device /dev/%I -b 115200 --telnet-gdb 127.0.0.1:5551
```

```
gdb-multiarch -iex "set auto-load safe-path scripts/gdb/" -iex "target remote 127.0.0.1:5551" ./vmlinux
```

These can be combined with `--telnet` and/or `--pty` for parallel access
to the console.



Websocket support
-----------------

Browsers don't have (normally) direct access to raw TCP sockets. To support
browser based telnet clients, it is possible to either use tools like
`websockify` or the native websocket support from tty2pty-mux:

```
tty2pty-mux --device /dev/%I -b 115200 --ws 127.0.0.1:8001
```

For quick tests, `websocat` is enough but it is recommended to just a proper
telnet client:

```
websocat ws://127.0.0.1:8001/
```
