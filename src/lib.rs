#![allow(dead_code)]

use crate::state::{Checksum, Invalid, Linefeed, MsgType, Params, Start, State, Talker, LF};
use bytes::BytesMut;
use std::mem::take;
use std::rc::Rc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder, Framed};

mod state;

const MAX_MSG_SIZE: usize = 82;

pub struct Context {
    current_state: Rc<Box<dyn State>>,
    inner: InnerContext,
}

impl Context {
    pub fn new() -> Self {
        let inner = InnerContext::new();

        Self {
            current_state: Rc::clone(&inner.states.start),
            inner,
        }
    }

    pub fn handle_event(&mut self, event: &u8) -> Result<Option<Nmea0183Msg>, String> {
        self.inner.event_count += 1;
        if self.inner.event_count > MAX_MSG_SIZE {
            let result = if self.inner.error.is_empty() {
                Err("Message too long".to_string())
            } else {
                Err(format!("{} + Message too long", &self.inner.error))
            };
            self.inner.reset();
            self.current_state = Rc::clone(&self.inner.states.start);
            result
        } else if *event == LF {
            let result = if self.inner.error.is_empty() {
                Ok(Some(take(&mut self.inner.msg)))
            } else {
                Err(take(&mut self.inner.error))
            };
            self.inner.reset();
            self.current_state = Rc::clone(&self.inner.states.start);
            result
        } else {
            self.current_state = self.current_state.handle_event(event, &mut self.inner);
            Ok(None)
        }
    }
}

struct InnerContext {
    event_count: usize,
    error: String,
    msg: Nmea0183Msg,
    states: StateList,
    chksum: u8,
    collect: String,
}

impl InnerContext {
    fn new() -> Self {
        Self {
            event_count: 0,
            error: String::new(),
            msg: Nmea0183Msg::default(),
            states: StateList::new(),
            chksum: 0,
            collect: String::new(),
        }
    }

    fn reset(&mut self) {
        self.msg = Nmea0183Msg::default();
        self.error.clear();
        self.event_count = 0;
        self.chksum = 0;
    }
}

struct StateList {
    start: Rc<Box<dyn State>>,
    talker: Rc<Box<dyn State>>,
    invalid: Rc<Box<dyn State>>,
    msgtype: Rc<Box<dyn State>>,
    params: Rc<Box<dyn State>>,
    chksum: Rc<Box<dyn State>>,
    linefeed: Rc<Box<dyn State>>,
}

impl StateList {
    fn new() -> Self {
        Self {
            start: Rc::new(Box::new(Start)),
            talker: Rc::new(Box::new(Talker)),
            invalid: Rc::new(Box::new(Invalid)),
            msgtype: Rc::new(Box::new(MsgType)),
            params: Rc::new(Box::new(Params)),
            chksum: Rc::new(Box::new(Checksum)),
            linefeed: Rc::new(Box::new(Linefeed)),
        }
    }
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

pub struct LineCodec {
    ctx: Context,
}

impl Default for LineCodec {
    fn default() -> Self {
        Self {
            ctx: Context::new(),
        }
    }
}

impl Decoder for LineCodec {
    type Item = Nmea0183Msg;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut rc = Ok(None);
        let offset = self.ctx.inner.event_count;

        // eprintln!("decode({}, offset: {})", to_string(src), offset);
        let position = src[offset..]
            .as_ref()
            .iter()
            .position(|b| match self.ctx.handle_event(b) {
                Ok(result) => {
                    if let Some(result) = result {
                        rc = Ok(Some(result));
                        true
                    } else {
                        false
                    }
                }
                Err(error) => {
                    rc = Err(std::io::Error::new(std::io::ErrorKind::Other, error));
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
    type Error = std::io::Error;
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
    use crate::state::CR;
    use std::fs::File;
    use std::io::BufRead;
    use std::path::Path;

    fn read_lines<P>(filename: P) -> std::io::Result<std::io::Lines<std::io::BufReader<File>>>
    where
        P: AsRef<Path>,
    {
        let file = File::open(filename)?;
        Ok(std::io::BufReader::new(file).lines())
    }

    #[test]
    fn test_data() {
        const TEST_FILE: &str = "./test_data/nmea0183_1000.log";
        let mut ctx = Context::new();
        match read_lines(TEST_FILE) {
            Ok(lines) => {
                // Consumes the iterator, returns an (Optional) String
                for (idx, line) in lines.enumerate() {
                    if let Ok(line) = line {
                        // println!("{} {}", idx + 1, line);
                        line.chars()
                            .for_each(|ch| match ctx.handle_event(&(ch as u8)) {
                                Ok(res) => {
                                    assert!(res.is_none(), "{} {}", idx + 1, line);
                                }
                                Err(error) => {
                                    panic!(
                                        "error parsing message {} {}: {:?}",
                                        idx + 1,
                                        line,
                                        error
                                    );
                                }
                            });
                        match ctx.handle_event(&CR) {
                            Ok(res) => {
                                assert!(res.is_none(), "{} {}", idx + 1, line)
                            }
                            Err(error) => {
                                panic!("error parsing message {} {}: {:?}", idx, line, error);
                            }
                        }
                        match ctx.handle_event(&LF) {
                            Ok(res) => {
                                assert!(
                                    res.expect(
                                        format!("empty result @{}, {}", idx + 1, line).as_str()
                                    )
                                    .chksum_valid
                                    .expect(
                                        format!("checksum not calculated @{} {}", idx + 1, line)
                                            .as_str()
                                    ),
                                    "checksum does not match @{} {}",
                                    idx + 1,
                                    line
                                )
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
}
