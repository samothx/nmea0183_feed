use crate::nmea0183_codec::context::state::{
    Checksum, Invalid, Linefeed, MsgType, Params, Start, State, Talker, LF,
};
use crate::Nmea0183Msg;
use std::mem::take;
use std::rc::Rc;

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
        /*eprintln!(
            "handle_event({}) in state {}",
            byte_2_print(event),
            self.current_state.name()
        );*/
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
        } else {
            self.current_state = self.current_state.handle_event(event, &mut self.inner);
            if *event == LF {
                let result = if self.inner.error.is_empty() {
                    Ok(Some(take(&mut self.inner.msg)))
                } else {
                    Err(take(&mut self.inner.error))
                };
                self.inner.reset();
                self.current_state = Rc::clone(&self.inner.states.start);
                result
            } else {
                Ok(None)
            }
        }
    }
    pub fn get_event_count(&self) -> usize {
        self.inner.event_count
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

#[cfg(test)]
mod tests {
    extern crate test;

    const CR: u8 = 0xD;
    const LF: u8 = 0xA;

    use super::*;
    // use crate::state::CR;
    use std::fs::{read_to_string, File};
    use std::io::BufRead;
    use std::path::Path;

    use test::Bencher;

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

    #[bench]
    fn bench_data(b: &mut Bencher) {
        const TEST_FILE: &str = "./test_data/nmea0183_1000.log";
        let test_data =
            read_to_string(TEST_FILE).expect(format!("failed to open file {}", TEST_FILE).as_str());

        b.iter(|| {
            let mut ctx = Context::new();
            let mut count = 0;
            test_data.chars().for_each(|ch| {
                if let Ok(res) = ctx.handle_event(&(ch as u8)) {
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
