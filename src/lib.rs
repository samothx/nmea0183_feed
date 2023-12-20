//#![allow(dead_code)]

use bytes::BytesMut;
use std::io;
use std::mem::take;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};

#[derive(PartialEq, Debug, Clone)]
enum Nmea0168State {
    Encapsulation,   // nothing received
    Talker,          // waiting for talker bytes
    MsgType,         // waiting for msgtype bytes
    Params,          // waiting for parameter bytes
    Chksum,          // waiting for checksum bytes
    Invalid(String), // invalid - waiting for anything
    LF,              // waiting for lf on valid message
}

#[derive(Debug)]
pub struct Nmea0168Msg {
    encapsulation: bool,
    talker: String,
    msgtype: String,
    params: Vec<String>,
    chksum: String,
}

impl Nmea0168Msg {}

impl Default for Nmea0168Msg {
    fn default() -> Self {
        Self {
            encapsulation: false,
            talker: "".to_string(),
            msgtype: "".to_string(),
            params: Vec::new(),
            chksum: String::new(),
        }
    }
}

const LF: u8 = 0xA;
const CR: u8 = 0xD;
const AST: u8 = b'*';
const XCL: u8 = b'!';
const START: u8 = b'$';
//const RES: u8 = b'~';
const FIELD: u8 = b',';
// const TAG: u8 = b'\\';
// const HEX: u8 = b'^';

struct Nmea0168Stm {
    msg: Nmea0168Msg,
    param: String,
    count: usize,
    state: Nmea0168State,
}

impl Nmea0168Stm {
    fn new() -> Self {
        Self {
            msg: Nmea0168Msg::default(),
            count: 0,
            state: Nmea0168State::Encapsulation,
            param: String::new(),
        }
    }

    fn add_byte(&mut self, byte: &u8) -> Result<Option<Nmea0168Msg>, String> {
        // let state = self.state.clone();

        if let Nmea0168State::Invalid(_) = self.state {
        } else {
            // check for max message size
            self.count += 1;
            if self.count > 82 {
                self.state =
                    Nmea0168State::Invalid("Message length exceeds 82 characters".to_string());
            }
        }

        if *byte == LF {
            if self.state == Nmea0168State::LF {
                self.state = Nmea0168State::Encapsulation;
                self.count = 0;
                let result = take(&mut self.msg);
                // eprintln!("add_byte({} {} in {:?}) -> {:?},{:?}", byte, if ((*byte) as char).is_control() { '?' } else { (*byte) as char }, state, self.state, Some(&result));
                Ok(Some(result))
            } else {
                let err_msg = if let Nmea0168State::Invalid(err_msg) = &self.state {
                    err_msg.clone()
                } else {
                    format!("Unexpected line feed in state {:?}", self.state)
                };
                self.state = Nmea0168State::Encapsulation;
                self.msg = Nmea0168Msg::default();
                self.count = 0;
                // eprintln!("add_byte({} {} in {:?}) -> {:?},None", byte, if ((*byte) as char).is_control() { '?' } else { (*byte) as char }, state, self.state);
                Err(err_msg)
            }
        } else {
            match self.state {
                Nmea0168State::Encapsulation => match *byte {
                    XCL => {
                        self.msg.encapsulation = true;
                        self.state = Nmea0168State::Talker;
                    }
                    START => {
                        self.msg.encapsulation = false;
                        self.state = Nmea0168State::Talker;
                    }
                    _ => {
                        self.state = Nmea0168State::Invalid(format!(
                            "Unexpected {:02X}-{} in state {:?}",
                            *byte,
                            if ((*byte) as char).is_control() {
                                '?'
                            } else {
                                (*byte) as char
                            },
                            self.state
                        ))
                    }
                },
                Nmea0168State::Talker => match *byte {
                    b'A'..=b'Z' => {
                        self.msg.talker.push((*byte) as char);
                        if self.msg.talker.len() > 1 {
                            self.state = Nmea0168State::MsgType;
                        }
                    }
                    _ => {
                        self.state = Nmea0168State::Invalid(format!(
                            "Unexpected {:02X}-{} in state {:?}",
                            *byte,
                            if ((*byte) as char).is_control() {
                                '?'
                            } else {
                                (*byte) as char
                            },
                            self.state
                        ))
                    }
                },
                Nmea0168State::MsgType => match *byte {
                    b'A'..=b'Z' => {
                        if self.msg.msgtype.len() < 3 {
                            self.msg.msgtype.push((*byte) as char);
                        } else {
                            self.state = Nmea0168State::Invalid(format!(
                                "Unexpected {:02X}-{} in state {:?}",
                                *byte,
                                if ((*byte) as char).is_control() {
                                    '?'
                                } else {
                                    (*byte) as char
                                },
                                self.state
                            ))
                        }
                    }
                    FIELD => {
                        if self.msg.msgtype.len() == 3 {
                            self.state = Nmea0168State::Params;
                        } else {
                            self.state = Nmea0168State::Invalid(format!(
                                "Unexpected {:02X}-{} in state {:?}",
                                *byte,
                                if ((*byte) as char).is_control() {
                                    '?'
                                } else {
                                    (*byte) as char
                                },
                                self.state
                            ))
                        }
                    }
                    AST => {
                        if self.msg.msgtype.len() == 3 {
                            self.state = Nmea0168State::Chksum;
                        } else {
                            self.state = Nmea0168State::Invalid(format!(
                                "Unexpected {:02X}-{} in state {:?}",
                                *byte,
                                if ((*byte) as char).is_control() {
                                    '?'
                                } else {
                                    (*byte) as char
                                },
                                self.state
                            ))
                        }
                    }
                    CR => {
                        if self.msg.msgtype.len() == 3 {
                            self.state = Nmea0168State::LF;
                        } else {
                            self.state = Nmea0168State::Invalid(format!(
                                "Unexpected {:02X}-{} in state {:?}",
                                *byte,
                                if ((*byte) as char).is_control() {
                                    '?'
                                } else {
                                    (*byte) as char
                                },
                                self.state
                            ))
                        }
                    }
                    _ => {
                        self.state = Nmea0168State::Invalid(format!(
                            "Unexpected {:02X}-{} in state {:?}",
                            *byte,
                            if ((*byte) as char).is_control() {
                                '?'
                            } else {
                                (*byte) as char
                            },
                            self.state
                        ))
                    }
                },
                Nmea0168State::Params => {
                    // TODO: exclude XCL,START => invalid
                    // TODO: handle TAG, HEX
                    match *byte {
                        FIELD => self.msg.params.push(take(&mut self.param)),
                        AST => {
                            self.msg.params.push(take(&mut self.param));
                            self.state = Nmea0168State::Chksum;
                        }
                        CR => {
                            // TODO: does there have to be a comma before CR ?
                            self.msg.params.push(take(&mut self.param));
                            self.state = Nmea0168State::LF
                        }
                        LF => {
                            // TODO: does there have to be a comma before CR ?
                            self.msg.params.push(take(&mut self.param));
                            self.state = Nmea0168State::LF
                        }

                        _ => self.param.push((*byte) as char),
                    }
                }
                Nmea0168State::Chksum => match *byte {
                    b'A'..=b'F' | b'0'..=b'9' => {
                        if self.msg.chksum.len() < 2 {
                            self.msg.chksum.push((*byte) as char)
                        } else {
                            self.state = Nmea0168State::Invalid(format!(
                                "Unexpected {:02X}-{} in state {:?}",
                                *byte,
                                if ((*byte) as char).is_control() {
                                    '?'
                                } else {
                                    (*byte) as char
                                },
                                self.state
                            ))
                        }
                    }
                    CR => {
                        if self.msg.chksum.len() == 2 {
                            self.state = Nmea0168State::LF;
                        } else {
                            self.state = Nmea0168State::Invalid(format!(
                                "Unexpected {:02X}-{} in state {:?}",
                                *byte,
                                if ((*byte) as char).is_control() {
                                    '?'
                                } else {
                                    (*byte) as char
                                },
                                self.state
                            ))
                        }
                    }
                    _ => {
                        self.state = Nmea0168State::Invalid(format!(
                            "Unexpected {:02X}-{} in state {:?}",
                            *byte,
                            if ((*byte) as char).is_control() {
                                '?'
                            } else {
                                (*byte) as char
                            },
                            self.state
                        ))
                    }
                },
                Nmea0168State::LF => {
                    if *byte != LF {
                        self.state = Nmea0168State::Invalid(format!(
                            "Unexpected {:02X}-{} in state {:?}",
                            *byte,
                            if ((*byte) as char).is_control() {
                                '?'
                            } else {
                                (*byte) as char
                            },
                            self.state
                        ))
                    }
                }
                Nmea0168State::Invalid(_) => {}
            };

            /* eprintln!(
                "add_byte({} {} in {:?}) -> {:?},None",
                byte,
                if ((*byte) as char).is_control() {
                    '?'
                } else {
                    (*byte) as char
                },
                state,
                self.state
            ); */

            Ok(None)
        }
    }
}

pub struct LineCodec {
    stm: Nmea0168Stm,
}

impl Default for LineCodec {
    fn default() -> Self {
        Self {
            stm: Nmea0168Stm::new(),
        }
    }
}

impl Decoder for LineCodec {
    type Item = Nmea0168Msg;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut result = None;
        let position = src.as_ref().iter().position(|b| {
            if let Ok(res) = self.stm.add_byte(b) {
                if let Some(msg) = res {
                    result = Some(msg);
                    true
                } else {
                    false
                }
            } else {
                true
            }
        });

        if let Some(position) = position {
            _ = src.split_to(position + 1);
        } else {
            _ = src.split();
        }
        Ok(result)
    }
}

impl Encoder<String> for LineCodec {
    type Error = io::Error;
    fn encode(&mut self, _item: String, _dst: &mut BytesMut) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub fn get_codec<T>(port: T) -> Framed<impl AsyncRead, LineCodec>
where
    T: AsyncRead + AsyncWrite + Sized,
{
    // let mut codec = ;
    LineCodec::default().framed(port)
}

#[allow(dead_code)]
fn to_string(buf: &BytesMut) -> String {
    let mut str_buf = String::new();
    buf.iter().for_each(|byte| {
        str_buf.push_str(
            format!(
                "{:02x} {},",
                byte,
                if ((*byte) as char).is_control() {
                    '?'
                } else {
                    (*byte) as char
                }
            )
            .as_str(),
        )
    });
    str_buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
