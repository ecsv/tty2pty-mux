// SPDX-License-Identifier: GPL-2.0

use std::{
    borrow::Cow,
    io,
};
use tokio::{
    io::{
        AsyncReadExt,
        AsyncWriteExt,
    },
    sync::{
        broadcast,
        mpsc,
    },
    time::{
        Duration,
        sleep,
    },
};
use tokio_serial::{
    SerialPort,
    SerialPortBuilderExt,
};

use crate::TtyMsg;

pub fn attach(
    serial_path: Cow<'_, str>,
    baudrate: u32,
    mux_tx: broadcast::Sender<Vec<u8>>,
    mut mux_rx: mpsc::Receiver<TtyMsg>,
) -> Result<tokio::task::JoinHandle<()>, std::io::Error> {
    let mut serial = tokio_serial::new(serial_path, baudrate)
        .data_bits(tokio_serial::DataBits::Eight)
        .parity(tokio_serial::Parity::None)
        .stop_bits(tokio_serial::StopBits::One)
        .flow_control(tokio_serial::FlowControl::None)
        .exclusive(true)
        .open_native_async()
        .map_err(io::Error::other)?;

    let t_serial = tokio::spawn(async move {
        let mut buf = [0u8; crate::BUF_SIZE];
        loop {
            tokio::select! {
                // copy from serial to all telnet/PTYs
                msg = serial.read(&mut buf) => {
                    match msg {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if mux_tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }

                // copy from telnet/PTYs to serial
                rx = mux_rx.recv() => {
                    if let Some(msg) = rx {
                        match msg {
                            TtyMsg::Data(data) => {
                                if serial.write_all(&data).await.is_err() {
                                    break;
                                }
                            },
                            TtyMsg::Break => {
                                let _ = serial.set_break();
                                sleep(Duration::from_millis(250)).await;
                                let _ = serial.clear_break();
                            },
                        }
                    }
                }
            }
        }

        log::warn!("Serial closed");
    });

    Ok(t_serial)
}
