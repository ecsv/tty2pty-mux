// SPDX-License-Identifier: GPL-2.0

mod pty;
mod telnet;
mod tty;

use clap::{
    CommandFactory,
    Parser,
    error::ErrorKind,
};
use std::{
    io,
    net::SocketAddr,
    path::PathBuf,
};
use tokio::sync::{
    broadcast,
    mpsc,
};

const BROADCAST_CAP: usize = 256;
const BUF_SIZE: usize = 4096;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short = 'D', long, default_value = "/dev/ttyUSB0")]
    device: PathBuf,

    #[arg(short, long, default_value_t = 115_200)]
    baudrate: u32,

    #[arg(short, long)]
    pty: Vec<PathBuf>,

    #[arg(short = 'P', long = "pty-gdb")]
    pty_gdb: Vec<PathBuf>,

    #[arg(short, long)]
    telnet: Vec<SocketAddr>,

    #[arg(short = 'T', long = "telnet-gdb")]
    telnet_gdb: Vec<SocketAddr>,
}

enum TtyMsg {
    Data(Vec<u8>),
    Break,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    env_logger::init();
    let args = Args::parse();

    if args.pty.is_empty()
        && args.pty_gdb.is_empty()
        && args.telnet.is_empty()
        && args.telnet_gdb.is_empty()
    {
        Args::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "at least one of --pty, --pty-gdb, --telnet, or --telnet-gdb must be specified",
            )
            .exit();
    }

    // create channels to copy data between serial and telnet/pty endpoints
    //
    // * many telnet/pty endpoints can write to a single serial
    // * a single serial writes to multiple telnet/pty endpoints
    let (mux_tx, serial_rx) = broadcast::channel::<Vec<u8>>(BROADCAST_CAP);
    let (serial_tx, mux_rx) = mpsc::channel::<TtyMsg>(BROADCAST_CAP);

    // connect to serial
    let t_serial = tty::attach(args.device.to_string_lossy(), args.baudrate, mux_tx, mux_rx)?;

    // create pty and telnet endpoints
    for link in &args.pty {
        pty::spawn(link, serial_rx.resubscribe(), serial_tx.clone(), false)?;
    }

    for link in &args.pty_gdb {
        pty::spawn(link, serial_rx.resubscribe(), serial_tx.clone(), true)?;
    }

    for addr in &args.telnet {
        telnet::serve(addr, serial_rx.resubscribe(), serial_tx.clone(), false).await?;
    }

    for addr in &args.telnet_gdb {
        telnet::serve(addr, serial_rx.resubscribe(), serial_tx.clone(), true).await?;
    }

    // Wait until serial broken (disconnected) or user send Ctrl-C
    let ctrlc = tokio::signal::ctrl_c();
    tokio::select! {
        _ = t_serial  => {},
        _ = ctrlc => {
            log::info!("Ctrl-C received, cleaning up...");
        },
    }

    // clean up symlinks from PTY endpoints
    for link in &args.pty {
        if link.exists() {
            pty::remove_symlink(link);
        }
    }

    Ok(())
}
