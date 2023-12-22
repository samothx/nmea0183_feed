#![allow(dead_code)]
#![feature(test)]

use crate::nmea0183_codec::Nmea0183Codec;
use bytes::BytesMut;
// use crate::state::{Checksum, Invalid, Linefeed, MsgType, Params, Start, State, Talker, LF};
// use bytes::BytesMut;
// use std::mem::take;
// use std::rc::Rc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Framed};

mod nmea0183_codec;

#[derive(Debug)]
pub struct Nmea0183Msg {
    encapsulation: bool,
    talker: String,
    msgtype: String,
    params: Vec<String>,
    chksum: String,
    chksum_valid: Option<bool>,
}

impl Default for Nmea0183Msg {
    fn default() -> Self {
        Self {
            encapsulation: false,
            talker: "".to_string(),
            msgtype: "".to_string(),
            params: Vec::new(),
            chksum: String::new(),
            chksum_valid: None,
        }
    }
}

pub fn get_codec<T>(port: T) -> Framed<impl AsyncRead, Nmea0183Codec>
where
    T: AsyncRead + AsyncWrite + Sized,
{
    // let mut codec = ;
    Nmea0183Codec::default().framed(port)
}

#[allow(dead_code)]
fn byte_2_print(byte: &u8) -> String {
    format!(
        "{:02X}-{}",
        byte,
        if ((*byte) as char).is_control() {
            '☺'
        } else {
            (*byte) as char
        }
    )
}

#[allow(dead_code)]
fn to_string(buf: &BytesMut) -> String {
    let mut str_buf = String::new();
    buf.iter()
        .for_each(|byte| str_buf.push_str(format!("{},", byte_2_print(byte)).as_str()));
    str_buf
}

#[cfg(test)]
mod tests {
    use crate::get_codec;
    use futures::stream::StreamExt;
    use tokio::fs::File;
    use tokio::io::AsyncReadExt;
    use tokio_test;

    const TEST_FILE: &str = "./test_data/nmea0183_1000.log";

    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    #[test]
    fn test_ok() {
        aw!(async {
            let file = File::open(TEST_FILE)
                .await
                .expect(format!("failed to open file {}", TEST_FILE).as_str());

            let mut reader = get_codec(file);
            let mut count = 0;
            while let Some(result) = reader.next().await {
                count += 1;
                match result {
                    Ok(res) => {
                        eprintln!("{:?}", res)
                    }
                    Err(error) => {
                        panic!("Error on msg {}: {:?}", count, error)
                    }
                }
            }
        })
    }

    #[test]
    fn test_first_fail() {
        aw!(async {
            let mut file = File::open(TEST_FILE)
                .await
                .expect(format!("failed to open file {}", TEST_FILE).as_str());

            let mut buf: [u8; 1] = [0];
            file.read(&mut buf)
                .await
                .expect(format!("failed to read from file {}", TEST_FILE).as_str());

            let mut reader = get_codec(file);
            let mut count = 0;
            while let Some(result) = reader.next().await {
                count += 1;
                match result {
                    Ok(res) => eprintln!("{:?}", res),
                    Err(error) => {
                        panic!("Error on msg {}: {:?}", count, error)
                    }
                }
            }
        })
    }
}
