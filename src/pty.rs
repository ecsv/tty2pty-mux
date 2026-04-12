// SPDX-License-Identifier: GPL-2.0

use futures::StreamExt;
use std::{
    io,
    os::{
        fd::AsFd,
        unix::io::AsRawFd,
    },
    path::{
        Path,
        PathBuf,
    },
};
use tokio::{
    io::AsyncWriteExt,
    sync::{
        broadcast,
        mpsc,
    },
};
use tokio_util::{
    bytes::{
        Buf,
        Bytes,
        BytesMut,
    },
    codec::{
        Decoder,
        Framed,
    },
};

use crate::{
    TtyMsg,
    telnet::queue_gdb_break,
};

#[derive(Debug)]
pub enum PtyByteEvent {
    Data(Bytes),
    GdbInterrupt,
}

#[derive(Debug)]
pub struct PtyByteCodec {
    interrupt_as_break: bool,
}

impl PtyByteCodec {
    #[must_use]
    pub fn new(interrupt_as_break: bool) -> Self {
        PtyByteCodec { interrupt_as_break }
    }
}

impl Decoder for PtyByteCodec {
    type Error = std::io::Error;
    type Item = PtyByteEvent;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<PtyByteEvent>, std::io::Error> {
        if buf.is_empty() {
            return Ok(None);
        }

        let interrupt_offset = if self.interrupt_as_break {
            buf.iter().position(|b| *b == 0x03)
        } else {
            None
        };

        let ret = match interrupt_offset {
            None => {
                let bytes = Bytes::from(buf.split());
                PtyByteEvent::Data(bytes)
            }
            Some(0) => {
                buf.advance(1);
                PtyByteEvent::GdbInterrupt
            }
            Some(pos) => {
                let bytes = Bytes::from(buf.split_to(pos));
                PtyByteEvent::Data(bytes)
            }
        };

        Ok(Some(ret))
    }
}

fn configure_pts_raw(pts: &pty_process::Pts) -> rustix::io::Result<()> {
    let fd = pts.as_fd();
    let mut attrs = rustix::termios::tcgetattr(fd)?;
    attrs.make_raw();
    rustix::termios::tcsetattr(fd, rustix::termios::OptionalActions::Now, &attrs)
}

fn resolve_pts_path(pts: &pty_process::Pts) -> io::Result<PathBuf> {
    let link = format!("/proc/self/fd/{}", pts.as_raw_fd());
    std::fs::read_link(link)
}

fn make_symlink(link: &std::path::Path, target: &std::path::Path) -> io::Result<()> {
    // Remove stale link if it exists (ignore error if absent)
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

pub fn remove_symlink(link: &Path) {
    if let Err(e) = std::fs::remove_file(link) {
        log::warn!("could not remove {}: {e}", link.display());
    }
}

pub fn spawn(
    link: &Path,
    mut serial_rx: broadcast::Receiver<Vec<u8>>,
    serial_tx: mpsc::Sender<TtyMsg>,
    interrupt_as_break: bool,
) -> io::Result<()> {
    let (pty, pts) = pty_process::open().map_err(io::Error::other)?;

    configure_pts_raw(&pts).map_err(io::Error::other)?;

    let pts_path = resolve_pts_path(&pts)?;
    log::info!("PTY[{}] link: {}", link.display(), pts_path.display());
    make_symlink(link, &pts_path)?;

    let (pty_rx, mut pty_tx) = pty.into_split();

    // copy from serial to this PTY
    let link_reader = link.to_path_buf();
    tokio::spawn(async move {
        loop {
            match serial_rx.recv().await {
                Ok(data) => {
                    if pty_tx.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("PTY[{}]: lagged by {n} messages", link_reader.display());
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
        log::warn!("PTY[{}] serial->pty done", link_reader.display());
    });

    // copy from this PTY to serial
    let link_tx = link.to_path_buf();
    tokio::spawn(async move {
        let _pts = pts; // keep slave fd open

        let codec = PtyByteCodec::new(interrupt_as_break);
        let mut frame = Framed::new(pty_rx, codec);

        loop {
            match frame.next().await {
                Some(Ok(msg)) => match msg {
                    PtyByteEvent::Data(buf) => {
                        if serial_tx.send(TtyMsg::Data(buf.to_vec())).await.is_err() {
                            break;
                        }
                    }
                    PtyByteEvent::GdbInterrupt => {
                        if queue_gdb_break(&serial_tx).await.is_err() {
                            break;
                        }
                    }
                },
                Some(Err(_)) => break,
                None => break,
            }
        }

        remove_symlink(&link_tx);
        log::warn!("PTY[{}] pty->serial done", link_tx.display());
    });

    Ok(())
}
