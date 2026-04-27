// SPDX-License-Identifier: GPL-2.0

mod commands;

use futures::{
    StreamExt,
    sink::SinkExt,
};
use std::io::{
    self,
};
use tokio::{
    io::{
        AsyncRead,
        AsyncWrite,
    },
    net::{
        TcpListener,
        ToSocketAddrs,
    },
    sync::{
        broadcast,
        mpsc,
    },
};
use tokio_util::{
    bytes::{
        Buf,
        BufMut,
        Bytes,
        BytesMut,
    },
    codec::{
        Decoder,
        Encoder,
        Framed,
    },
};

use crate::TtyMsg;

#[derive(Debug)]
pub enum TelnetByteOption {
    Echo,
    SuppressGoAhead,
    Binary,
    Unsupported(u8),
}

#[derive(Debug)]
pub enum TelnetByteEvent {
    Data(Bytes),
    Break,
    Will(TelnetByteOption),
    Wont(TelnetByteOption),
    Do(TelnetByteOption),
    Dont(TelnetByteOption),
    GdbInterrupt,
}

impl From<u8> for TelnetByteOption {
    fn from(byte: u8) -> Self {
        match byte {
            commands::OPTION_ECHO => TelnetByteOption::Echo,
            commands::OPTION_SGA => TelnetByteOption::SuppressGoAhead,
            commands::OPTION_BINARY => TelnetByteOption::Binary,
            _ => TelnetByteOption::Unsupported(byte),
        }
    }
}

impl From<TelnetByteOption> for u8 {
    fn from(option: TelnetByteOption) -> Self {
        match option {
            TelnetByteOption::Echo => commands::OPTION_ECHO,
            TelnetByteOption::SuppressGoAhead => commands::OPTION_SGA,
            TelnetByteOption::Binary => commands::OPTION_BINARY,
            TelnetByteOption::Unsupported(byte) => byte,
        }
    }
}

#[derive(Debug)]
pub struct TelnetByteCodec {
    interrupt_as_break: bool,
}

impl TelnetByteCodec {
    #[must_use]
    pub fn new(interrupt_as_break: bool) -> Self {
        TelnetByteCodec { interrupt_as_break }
    }
}

impl Encoder<TelnetByteEvent> for TelnetByteCodec {
    type Error = std::io::Error;

    fn encode(&mut self, event: TelnetByteEvent, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        match event {
            TelnetByteEvent::Data(msg) => encode_data(msg, buffer),
            TelnetByteEvent::Will(option) => {
                buffer.extend([commands::IAC, commands::WILL, option.into()])
            }
            TelnetByteEvent::Wont(option) => {
                buffer.extend([commands::IAC, commands::WONT, option.into()])
            }
            TelnetByteEvent::Do(option) => {
                buffer.extend([commands::IAC, commands::DO, option.into()])
            }
            TelnetByteEvent::Dont(option) => {
                buffer.extend([commands::IAC, commands::DONT, option.into()])
            }
            _ => {}
        }

        Ok(())
    }
}

fn encode_data(bytes: Bytes, buffer: &mut BytesMut) {
    let mut bytes_buffer_size = bytes.len();

    bytes_buffer_size += bytes.iter().filter(|&x| *x == commands::IAC).count();
    buffer.reserve(bytes_buffer_size);

    for byte in &bytes {
        if *byte == commands::IAC {
            buffer.put_u8(commands::IAC);
        }
        buffer.put_u8(*byte);
    }
}

impl TelnetByteCodec {
    fn decode_special_char(&mut self, buf: &mut BytesMut) -> Option<TelnetByteEvent> {
        if self.interrupt_as_break && buf[0] == 0x03 {
            buf.advance(1);
            return Some(TelnetByteEvent::GdbInterrupt);
        }

        if buf[0] != commands::IAC {
            buf.advance(1);
            return None;
        }

        if buf.len() < 2 {
            return None;
        }

        if buf[1] == commands::IAC {
            buf.advance(2);
            return Some(TelnetByteEvent::Data(Bytes::from(vec![commands::IAC])));
        }

        if buf[1] == commands::BREAK {
            buf.advance(2);
            return Some(TelnetByteEvent::Break);
        }

        if buf.len() < 3 {
            return None;
        }

        let command = buf.split_to(3);

        let cmd_event = match command[1] {
            commands::WILL => TelnetByteEvent::Will(TelnetByteOption::from(command[2])),
            commands::WONT => TelnetByteEvent::Wont(TelnetByteOption::from(command[2])),
            commands::DO => TelnetByteEvent::Do(TelnetByteOption::from(command[2])),
            commands::DONT => TelnetByteEvent::Dont(TelnetByteOption::from(command[2])),
            _ => TelnetByteEvent::Data(Bytes::new()),
        };

        Some(cmd_event)
    }
}

impl Decoder for TelnetByteCodec {
    type Error = std::io::Error;
    type Item = TelnetByteEvent;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<TelnetByteEvent>, std::io::Error> {
        if buf.is_empty() {
            return Ok(None);
        }

        let control_offset = buf
            .iter()
            .position(|&b| b == commands::IAC || (self.interrupt_as_break && b == 0x03));

        let ret = match control_offset {
            None => {
                let bytes = Bytes::from(buf.split());
                Some(TelnetByteEvent::Data(bytes))
            }
            Some(0) => self.decode_special_char(buf),
            Some(pos) => {
                let bytes = Bytes::from(buf.split_to(pos));
                Some(TelnetByteEvent::Data(bytes))
            }
        };

        Ok(ret)
    }
}

pub async fn queue_gdb_break(
    serial_tx: &mpsc::Sender<TtyMsg>,
) -> Result<(), tokio::sync::mpsc::error::SendError<TtyMsg>> {
    serial_tx.send(TtyMsg::Break).await?;
    serial_tx.send(TtyMsg::Data(vec![0x67u8])).await?;

    Ok(())
}

pub async fn serve<A: ToSocketAddrs>(
    addr: A,
    serial_rx: broadcast::Receiver<Vec<u8>>,
    serial_tx: mpsc::Sender<TtyMsg>,
    interrupt_as_break: bool,
) -> io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tokio::spawn(async move {
        loop {
            let Ok((socket, client_addr)) = listener.accept().await else {
                continue;
            };
            let (socket_rx, socket_tx) = tokio::io::split(socket);
            serve_client(
                &serial_rx,
                &serial_tx,
                interrupt_as_break,
                socket_rx,
                socket_tx,
                client_addr,
            );
        }
    });

    Ok(())
}

fn serve_client<R: AsyncRead + Unpin + Send + 'static, W: AsyncWrite + Unpin + Send + 'static>(
    serial_rx: &broadcast::Receiver<Vec<u8>>,
    serial_tx: &mpsc::Sender<TtyMsg>,
    interrupt_as_break: bool,
    socket_rx: R,
    socket_tx: W,
    client_addr: std::net::SocketAddr,
) {
    let mut serial_rx_client = serial_rx.resubscribe();
    let serial_tx_client = serial_tx.clone();

    // copy from serial to this telnet client
    tokio::spawn(async move {
        let codec = TelnetByteCodec::new(interrupt_as_break);
        let mut frame = Framed::new(socket_tx, codec);

        // switch to binary, single byte without echo by telnet client
        let _ = frame
            .send(TelnetByteEvent::Will(TelnetByteOption::Echo))
            .await;
        let _ = frame
            .send(TelnetByteEvent::Will(TelnetByteOption::SuppressGoAhead))
            .await;
        let _ = frame
            .send(TelnetByteEvent::Do(TelnetByteOption::SuppressGoAhead))
            .await;
        let _ = frame
            .send(TelnetByteEvent::Will(TelnetByteOption::Binary))
            .await;
        let _ = frame
            .send(TelnetByteEvent::Do(TelnetByteOption::Binary))
            .await;

        loop {
            match serial_rx_client.recv().await {
                Ok(data) => {
                    if frame
                        .send(TelnetByteEvent::Data(Bytes::from(data)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("TCP {client_addr}: lagged by {n} messages");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // copy from this telnet client to serial
    tokio::spawn(async move {
        let codec = TelnetByteCodec::new(interrupt_as_break);
        let mut frame = Framed::new(socket_rx, codec);

        // on connect, directly send break + start kgdb
        if interrupt_as_break {
            let _ = queue_gdb_break(&serial_tx_client).await;
        }

        loop {
            match frame.next().await {
                Some(Ok(msg)) => match msg {
                    TelnetByteEvent::Data(buf)
                        if serial_tx_client
                            .send(TtyMsg::Data(buf.to_vec()))
                            .await
                            .is_err() =>
                    {
                        break;
                    }
                    TelnetByteEvent::GdbInterrupt
                        if queue_gdb_break(&serial_tx_client).await.is_err() =>
                    {
                        break;
                    }
                    TelnetByteEvent::Break
                        if serial_tx_client.send(TtyMsg::Break).await.is_err() =>
                    {
                        break;
                    }
                    _ => {}
                },
                Some(Err(_)) => break,
                None => break,
            }
        }
    });
}
