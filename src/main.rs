#![warn(rust_2018_idioms)]

use futures::stream::StreamExt;
use std::{env, str};

use nmea0183_feed::get_codec;

use tokio_serial::SerialPortBuilderExt;

#[cfg(unix)]
const DEFAULT_TTY: &str = "/dev/ttyUSB0";
#[cfg(windows)]
const DEFAULT_TTY: &str = "COM1";

#[tokio::main(flavor = "current_thread")]
async fn main() -> tokio_serial::Result<()> {
    let mut args = env::args();
    let tty_path = args.nth(1).unwrap_or_else(|| DEFAULT_TTY.into());

    let mut port = tokio_serial::new(tty_path, 4800).open_native_async()?;

    #[cfg(unix)]
    port.set_exclusive(false)
        .expect("Unable to set serial port exclusive to false");

    let mut reader = get_codec(port);
    let mut msg_count = 0;
    while let Some(line_result) = reader.next().await {
        match line_result {
            Ok(line) => {
                msg_count += 1;
                println!("{:6} {:?}", msg_count, line)
            }
            Err(err) => {
                println!("{:6} Error {:?}", msg_count, err)
            }
        }
    }

    Ok(())
}
