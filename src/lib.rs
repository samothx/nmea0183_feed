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
