use crate::InnerContext;
use std::mem::take;
use std::rc::Rc;

pub const LF: u8 = 0xA;
pub const CR: u8 = 0xD;
pub const AST: u8 = b'*';
pub const XCL: u8 = b'!';
pub const START: u8 = b'$';
pub const FIELD: u8 = b',';
//const RES: u8 = b'~';
// const TAG: u8 = b'\\';
// const HEX: u8 = b'^';

pub trait State {
    fn handle_event(&self, event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>>;
    fn is_term(&self) -> bool {
        false
    }
    fn name(&self) -> &str;
}

pub struct Start;
impl State for Start {
    fn handle_event(&self, event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>> {
        match *event {
            XCL => {
                ctx.msg.encapsulation = true;
                Rc::clone(&ctx.states.talker)
            }
            START => Rc::clone(&ctx.states.talker),
            _ => {
                ctx.error = format!(
                    "Invalid event {} in state {}",
                    byte_2_print(event),
                    self.name()
                );
                Rc::clone(&ctx.states.invalid)
            }
        }
    }

    fn name(&self) -> &str {
        "Start"
    }
}

pub struct Talker;

impl State for Talker {
    fn handle_event(&self, event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>> {
        match *event {
            b'A'..=b'Z' => {
                ctx.chksum = ctx.chksum ^ event;
                ctx.collect.push(*event as char);
                if ctx.collect.len() > 1 {
                    ctx.msg.talker = take(&mut ctx.collect);
                    Rc::clone(&ctx.states.msgtype)
                } else {
                    Rc::clone(&ctx.states.talker)
                }
            }
            _ => {
                ctx.error = format!(
                    "Invalid event {} in state {}",
                    byte_2_print(event),
                    self.name()
                );
                Rc::clone(&ctx.states.invalid)
            }
        }
    }

    fn name(&self) -> &str {
        "Talker"
    }
}

pub struct MsgType;

impl State for MsgType {
    fn handle_event(&self, event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>> {
        match *event {
            b'A'..=b'Z' => {
                if ctx.collect.len() < 3 {
                    ctx.chksum = ctx.chksum ^ event;
                    ctx.collect.push(*event as char);
                    Rc::clone(&ctx.states.msgtype)
                } else {
                    ctx.error = format!(
                        "Invalid event @{} in state {}, expected ',' or '*' or CR, got {}",
                        ctx.event_count,
                        self.name(),
                        byte_2_print(event)
                    );
                    Rc::clone(&ctx.states.invalid)
                }
            }
            FIELD => {
                if ctx.collect.len() == 3 {
                    ctx.chksum = ctx.chksum ^ event;
                    ctx.msg.msgtype = take(&mut ctx.collect);
                    Rc::clone(&ctx.states.params)
                } else {
                    ctx.error = format!(
                        "Invalid event @{} in state {}, expected 'A'-'Z', got {}",
                        ctx.event_count,
                        self.name(),
                        byte_2_print(event)
                    );
                    Rc::clone(&ctx.states.invalid)
                }
            }
            // TODO: handle CR event ?
            _ => {
                ctx.error = format!(
                    "Invalid event {} @{} in state {}",
                    byte_2_print(event),
                    ctx.event_count,
                    self.name()
                );
                Rc::clone(&ctx.states.invalid)
            }
        }
    }
    fn name(&self) -> &str {
        "MsgType"
    }
}

pub struct Params;

impl State for Params {
    fn handle_event(&self, event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>> {
        match *event {
            FIELD => {
                ctx.chksum = ctx.chksum ^ event;
                ctx.msg.params.push(take(&mut ctx.collect));
                Rc::clone(&ctx.states.params)
            }
            AST => {
                ctx.msg.params.push(take(&mut ctx.collect));
                Rc::clone(&ctx.states.chksum)
            }
            CR => {
                ctx.msg.params.push(take(&mut ctx.collect));
                Rc::clone(&ctx.states.linefeed)
            }
            _ => {
                ctx.chksum = ctx.chksum ^ event;
                ctx.collect.push(*event as char);
                Rc::clone(&ctx.states.params)
            }
        }
    }

    fn name(&self) -> &str {
        "Params"
    }
}

pub struct Checksum;

impl State for Checksum {
    fn handle_event(&self, event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>> {
        match *event {
            b'A'..=b'F' | b'0'..=b'9' => {
                if ctx.collect.len() < 2 {
                    ctx.collect.push(*event as char);
                    Rc::clone(&ctx.states.chksum)
                } else {
                    ctx.error = format!(
                        "Invalid event @{} in state {}, expected CR, got {}",
                        ctx.event_count,
                        self.name(),
                        byte_2_print(event)
                    );
                    Rc::clone(&ctx.states.invalid)
                }
            }
            CR => {
                if ctx.collect.len() == 2 {
                    ctx.msg.chksum = take(&mut ctx.collect);
                    Rc::clone(&ctx.states.linefeed)
                } else {
                    ctx.error = format!(
                        "Invalid event {} @{} in state {}, expected CR",
                        byte_2_print(event),
                        ctx.event_count,
                        self.name()
                    );
                    Rc::clone(&ctx.states.invalid)
                }
            }

            _ => {
                ctx.error = format!(
                    "Invalid event {} @{} in state {}",
                    byte_2_print(event),
                    ctx.event_count,
                    self.name()
                );
                Rc::clone(&ctx.states.invalid)
            }
        }
    }
    fn name(&self) -> &str {
        "Checksum"
    }
}

pub struct Linefeed;

impl State for Linefeed {
    fn handle_event(&self, event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>> {
        ctx.error = format!(
            "Invalid event {} @{} in state {}",
            byte_2_print(event),
            ctx.event_count,
            self.name()
        );
        Rc::clone(&ctx.states.invalid)
    }

    fn name(&self) -> &str {
        "Linefeed"
    }
}

pub struct Invalid;

impl State for Invalid {
    fn handle_event(&self, _event: &u8, ctx: &mut InnerContext) -> Rc<Box<dyn State>> {
        Rc::clone(&ctx.states.invalid)
    }

    fn name(&self) -> &str {
        "Invalid"
    }
}

fn byte_2_print(byte: &u8) -> String {
    format!(
        "{:02X}:'{}'",
        byte,
        if ((*byte) as char).is_control() {
            '☺'
        } else {
            (*byte) as char
        }
    )
}

/*
    pub fn handle_event(&mut self, event: &u8) -> Result<Option<Nmea0183Msg>, String> {
        let mut current_state = self.current_state.as_ref().expect("context has not been initialized");
        let next_state = current_state.handle_event(event, self);
        unimplemented!()


        if let Some(state) = &self.current_state {
            let next_state =
            if next_state.is_term() {
                self.current_state = Some(Rc::clone(&self.state_list.encapsulation));
                if self.error.is_empty() {
                    Ok(Some(take(&mut self.msg)))
                } else {
                    Err(take(&mut self.error))
                }
            } else {
                self.current_state = Some(next_state);
                Ok(None)
            }
        } else {
            let next_state = self.state_list.encapsulation.handle_event(event, &mut self);
            if next_state.is_term() {
                self.current_state = Some(Rc::clone(&self.state_list.encapsulation));
                if self.error.is_empty() {
                    Ok(Some(take(&mut self.msg)))
                } else {
                    Err(take(&mut self.error))
                }
            } else {
                self.current_state = Some(next_state);
                Ok(None)
            }
        }

    }

    fn reset(&mut self) {
        self.error.clear();
        self.event_count = 0;
        self.current_state = Some(Rc::clone(&self.state_list.encapsulation));
        self.msg = Nmea0183Msg::default()
    }

    fn init_state(&mut self) -> bool {
        self.event_count += 1;
        if self.event_count > MAX_MSG_SIZE {
            self.error.push_str("Message is too long");
            true
        } else {
            false
        }
    }
}

trait State {
    fn handle_event(&self, event: &u8, ctx: &mut MsgContext) -> Rc<Box<dyn State>>;
    fn is_term(&self) -> bool;
}

struct Term;
impl State for Term {
    fn handle_event(&self, event: &u8, ctx: &mut MsgContext) -> Rc<Box<dyn State>> {
        Rc::clone(&ctx.state_list.term)
    }
    fn is_term(&self) -> bool {
        true
    }
}


struct Start;
impl State for Start {
    fn handle_event(&self, event: &u8, ctx: &mut MsgContext) -> Rc<Box<dyn State>> {
        todo!()
    }
    fn is_term(&self) -> bool {
        false
    }
}

struct Talker;

impl State for Talker {
    fn handle_event(&self, event: &u8, ctx: &mut MsgContext) -> Rc<Box<dyn State>> {
        todo!()
    }

    fn is_term(&self) -> bool {
        false
    }

}

struct Invalid;

impl State for Invalid {
    fn handle_event(&self, event: &u8, ctx: &mut MsgContext) -> Rc<Box<dyn State>> {
        if ctx.init_state() {
            Rc::clone(&ctx.state_list.term)
        } else {
            match *event {
                LF => Rc::clone(&ctx.state_list.term),
                _ => Rc::clone(&ctx.state_list.invalid)
            }
        }
    }

    fn is_term(&self) -> bool {
        false
    }

}
*/
