#![allow(dead_code)]
#![feature(test)]

extern crate test;

use bytes::BytesMut;
use std::io;
use std::io::Error;
use std::mem::take;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};

#[derive(PartialEq, Debug, Clone)]
enum NmeaState {
    Encapsulation,   // nothing received
    Talker,          // waiting for talker bytes
    MsgType,         // waiting for msgtype bytes
    Params,          // waiting for parameter bytes
    Chksum,          // waiting for checksum bytes
    Invalid(String), // invalid - waiting for anything
    LF,              // waiting for lf on valid message
}

#[derive(Debug)]
pub struct Nmea0183Msg {
    encapsulation: bool,
    talker: String,
    msgtype: String,
    params: Vec<String>,
    chksum: String,
    chksum_valid: Option<bool>,
}

impl Nmea0183Msg {}

const LF: u8 = 0xA;
const CR: u8 = 0xD;
const AST: u8 = b'*';
const XCL: u8 = b'!';
const START: u8 = b'$';
const FIELD: u8 = b',';
//const RES: u8 = b'~';
// const TAG: u8 = b'\\';
// const HEX: u8 = b'^';

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

struct NmeaStm {
    msg: Nmea0183Msg,
    param: String,
    count: usize,
    state: NmeaState,
    chksum: u8,
}

impl NmeaStm {
    fn new() -> Self {
        Self {
            msg: Nmea0183Msg::default(),
            count: 0,
            state: NmeaState::Encapsulation,
            param: String::new(),
            chksum: 0,
        }
    }

    fn add_byte(&mut self, byte: &u8) -> Result<Option<Nmea0183Msg>, String> {
        // let state = self.state.clone();
        // let log_msg = format!("add_byte({}) in state {:?}", byte_2_print(byte), self.state);

        self.count += 1;
        if self.count > 82 {
            self.state = NmeaState::Encapsulation;
            self.count = 0;
            // eprintln!("{} => error message too long", log_msg);
            return Err("Message length exceeds 82 characters".to_string());
        }

        if *byte == LF {
            if self.state == NmeaState::LF {
                self.state = NmeaState::Encapsulation;
                self.count = 0;
                self.chksum = 0;
                let result = take(&mut self.msg);
                // eprintln!("{} => OK", log_msg);
                Ok(Some(result))
            } else {
                let err_msg = if let NmeaState::Invalid(err_msg) = &self.state {
                    err_msg.clone()
                } else {
                    format!("Unexpected line feed in state {:?}", self.state)
                };
                self.state = NmeaState::Encapsulation;
                self.msg = Nmea0183Msg::default();
                self.count = 0;
                // eprintln!("{} => ERR {}", log_msg, err_msg);
                Err(err_msg)
            }
        } else {
            match self.state {
                NmeaState::Encapsulation => match *byte {
                    XCL => {
                        self.msg.encapsulation = true;
                        self.state = NmeaState::Talker;
                    }
                    START => {
                        self.msg.encapsulation = false;
                        self.state = NmeaState::Talker;
                    }
                    _ => {
                        self.state = NmeaState::Invalid(format!(
                            "Unexpected {} in state {:?}",
                            byte_2_print(byte),
                            self.state
                        ))
                    }
                },
                NmeaState::Talker => match *byte {
                    b'A'..=b'Z' => {
                        self.chksum = self.chksum ^ *byte;
                        self.msg.talker.push((*byte) as char);
                        if self.msg.talker.len() > 1 {
                            self.state = NmeaState::MsgType;
                        }
                    }
                    _ => {
                        self.state = NmeaState::Invalid(format!(
                            "Unexpected {} in state {:?}",
                            byte_2_print(byte),
                            self.state
                        ))
                    }
                },
                NmeaState::MsgType => match *byte {
                    b'A'..=b'Z' => {
                        self.chksum = self.chksum ^ *byte;
                        if self.msg.msgtype.len() < 3 {
                            self.msg.msgtype.push((*byte) as char);
                        } else {
                            self.state = NmeaState::Invalid(format!(
                                "Unexpected {} in state {:?}",
                                byte_2_print(byte),
                                self.state
                            ))
                        }
                    }
                    FIELD => {
                        self.chksum = self.chksum ^ *byte;
                        if self.msg.msgtype.len() == 3 {
                            self.state = NmeaState::Params;
                        } else {
                            self.state = NmeaState::Invalid(format!(
                                "Unexpected {} in state {:?}",
                                byte_2_print(byte),
                                self.state
                            ))
                        }
                    }
                    AST => {
                        if self.msg.msgtype.len() == 3 {
                            self.state = NmeaState::Chksum;
                        } else {
                            self.state = NmeaState::Invalid(format!(
                                "Unexpected {} in state {:?}",
                                byte_2_print(byte),
                                self.state
                            ))
                        }
                    }
                    CR => {
                        if self.msg.msgtype.len() == 3 {
                            self.state = NmeaState::LF;
                        } else {
                            self.state = NmeaState::Invalid(format!(
                                "Unexpected {} in state {:?}",
                                byte_2_print(byte),
                                self.state
                            ))
                        }
                    }
                    _ => {
                        self.state = NmeaState::Invalid(format!(
                            "Unexpected {} in state {:?}",
                            byte_2_print(byte),
                            self.state
                        ))
                    }
                },
                NmeaState::Params => {
                    // TODO: exclude XCL,START => invalid
                    // TODO: handle TAG, HEX
                    match *byte {
                        FIELD => {
                            self.chksum = self.chksum ^ *byte;
                            self.msg.params.push(take(&mut self.param))
                        }
                        AST => {
                            self.msg.params.push(take(&mut self.param));
                            self.state = NmeaState::Chksum;
                        }
                        CR => {
                            // TODO: does there have to be a comma before CR ?
                            self.msg.params.push(take(&mut self.param));
                            self.state = NmeaState::LF
                        }
                        LF => {
                            // TODO: does there have to be a comma before CR ?
                            self.msg.params.push(take(&mut self.param));
                            self.state = NmeaState::LF
                        }
                        _ => {
                            self.chksum = self.chksum ^ *byte;
                            self.param.push((*byte) as char)
                        }
                    }
                }
                NmeaState::Chksum => match *byte {
                    b'A'..=b'F' | b'0'..=b'9' => {
                        if self.msg.chksum.len() < 2 {
                            self.msg.chksum.push((*byte) as char)
                        } else {
                            self.state = NmeaState::Invalid(format!(
                                "Unexpected {} in state {:?}",
                                byte_2_print(byte),
                                self.state
                            ))
                        }
                    }
                    CR => {
                        if self.msg.chksum.len() == 2 {
                            // eprintln!("checksum: {}/{:02X}", self.msg.chksum, self.chksum);
                            if !self.msg.chksum.is_empty() {
                                self.msg.chksum_valid = Some(
                                    self.msg.chksum.eq(format!("{:02X}", self.chksum).as_str()),
                                )
                            }
                            self.state = NmeaState::LF;
                        } else {
                            self.state = NmeaState::Invalid(format!(
                                "Unexpected {} in state {:?}",
                                byte_2_print(byte),
                                self.state
                            ))
                        }
                    }
                    _ => {
                        self.state = NmeaState::Invalid(format!(
                            "Unexpected {} in state {:?}",
                            byte_2_print(byte),
                            self.state
                        ))
                    }
                },
                NmeaState::LF => {
                    if *byte != LF {
                        self.state = NmeaState::Invalid(format!(
                            "Unexpected {} in state {:?}",
                            byte_2_print(byte),
                            self.state
                        ))
                    }
                }
                NmeaState::Invalid(_) => {}
            };

            // eprintln!("{} => state {:?}", log_msg, self.state);
            Ok(None)
        }
    }
}

pub struct LineCodec {
    stm: NmeaStm,
}

impl Default for LineCodec {
    fn default() -> Self {
        Self {
            stm: NmeaStm::new(),
        }
    }
}

impl Decoder for LineCodec {
    type Item = Nmea0183Msg;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut rc = Ok(None);
        let offset = self.stm.count;

        // eprintln!("decode({}, offset: {})", to_string(src), offset);
        let position = src[offset..]
            .as_ref()
            .iter()
            .position(|b| match self.stm.add_byte(b) {
                Ok(result) => {
                    if let Some(result) = result {
                        rc = Ok(Some(result));
                        true
                    } else {
                        false
                    }
                }
                Err(error) => {
                    rc = Err(Error::new(io::ErrorKind::Other, error));
                    true
                }
            });

        if let Some(position) = position {
            _ = src.split_to(offset + position + 1);
        }
        rc
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
    use super::*;
    use io::BufRead;
    use std::fs::{read_to_string, File};

    use std::path::Path;
    use test::Bencher;

    fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
    where
        P: AsRef<Path>,
    {
        let file = File::open(filename)?;
        Ok(io::BufReader::new(file).lines())
    }

    #[test]
    fn test_data() {
        const TEST_FILE: &str = "./test_data/nmea0183_1000.log";
        match read_lines(TEST_FILE) {
            Ok(lines) => {
                // Consumes the iterator, returns an (Optional) String
                let mut stm = NmeaStm::new();
                for (idx, line) in lines.enumerate() {
                    if let Ok(line) = line {
                        // println!("{} {}", idx + 1, line);
                        line.chars().for_each(|ch| match stm.add_byte(&(ch as u8)) {
                            Ok(res) => {
                                assert!(res.is_none(), "{} {}", idx + 1, line);
                            }
                            Err(error) => {
                                panic!("error parsing message {} {}: {:?}", idx + 1, line, error);
                            }
                        });
                        match stm.add_byte(&CR) {
                            Ok(res) => {
                                assert!(res.is_none(), "{} {}", idx + 1, line)
                            }
                            Err(error) => {
                                panic!("error parsing message {} {}: {:?}", idx, line, error);
                            }
                        }
                        match stm.add_byte(&LF) {
                            Ok(res) => {
                                assert!(res.is_some(), "{} {}", idx + 1, line)
                            }
                            Err(error) => {
                                panic!("error parsing message {} {}: {:?}", idx + 1, line, error);
                            }
                        }
                    }
                }
            }
            Err(error) => {
                panic!("Error opening test file {} : {:?}", TEST_FILE, error);
            }
        }
    }
    #[bench]
    fn bench_data(b: &mut Bencher) {
        const TEST_FILE: &str = "./test_data/nmea0183_1000.log";
        let test_data =
            read_to_string(TEST_FILE).expect(format!("failed to open file {}", TEST_FILE).as_str());

        b.iter(|| {
            let mut ctx = NmeaStm::new();
            let mut count = 0;
            test_data.chars().for_each(|ch| {
                if let Ok(res) = ctx.add_byte(&(ch as u8)) {
                    if let Some(msg) = res {
                        if let Some(valid) = msg.chksum_valid {
                            if valid {
                                count += 1;
                            } else {
                                panic!("invalid checksum encountered")
                            }
                        } else {
                            panic!("no checksum encountered")
                        }
                    }
                } else {
                    panic!("failed to decode")
                }
            })
        });
    }
}
