use crate::Nmea0183Msg;
use std::mem::take;

pub const MAX_SIZE: usize = 82;
pub const LF: u8 = 0xA;
pub const CR: u8 = 0xD;
pub const AST: u8 = b'*';
pub const XCL: u8 = b'!';
pub const START: u8 = b'$';
pub const FIELD: u8 = b',';
//const RES: u8 = b'~';
// const TAG: u8 = b'\\';
// const HEX: u8 = b'^';

struct Handler {
    func: for<'a> fn(&u8, &'a mut InnerContext, &str) -> &'a Handler,
    name: String,
}

struct Context<'a> {
    handler: Option<&'a Handler>,
    inner: InnerContext,
}

impl<'a> Context<'a> {
    fn new() -> Self {
        Self {
            inner: InnerContext::new(),
            handler: None,
        }
    }

    fn handle_event(&'a mut self, event: &u8) -> Result<Option<Nmea0183Msg>, String> {
        self.inner.event_count += 1;
        if self.inner.event_count > MAX_SIZE {
            Err("Maximum message size exceeded".to_string())
        } else {
            if let Some(handler) = self.handler {
                self.handler = Some((handler.func)(
                    event,
                    &mut self.inner,
                    handler.name.as_str(),
                ));
            } else {
                self.handler = Some((self.inner.msgtype_handler.func)(
                    event,
                    &mut self.inner,
                    "Start",
                ));
            }
            if *event == LF {
                if self.inner.error.is_empty() {
                    Ok(Some(take(&mut self.inner.msg)))
                } else {
                    Err(take(&mut self.inner.error))
                }
            } else {
                Ok(None)
            }
        }
    }
}

struct InnerContext {
    msg: Nmea0183Msg,
    error: String,
    event_count: usize,
    chksum: u8,
    collect: String,

    // start_handler: Handler,
    talker_handler: Handler,
    msgtype_handler: Handler,
    params_handler: Handler,
    chksum_handler: Handler,
    linefeed_handler: Handler,
    invalid_handler: Handler,
}

impl InnerContext {
    fn new() -> Self {
        Self {
            msg: Nmea0183Msg::default(),
            error: String::new(),
            event_count: 0,
            chksum: 0,
            collect: String::new(),

            talker_handler: Handler {
                name: "Talker".to_string(),
                func: handle_talker_event,
            },
            msgtype_handler: Handler {
                name: "MsgType".to_string(),
                func: handle_start_event,
            },
            params_handler: Handler {
                name: "Params".to_string(),
                func: handle_start_event,
            },
            chksum_handler: Handler {
                name: "Chksum".to_string(),
                func: handle_start_event,
            },
            linefeed_handler: Handler {
                name: "Linefeed".to_string(),
                func: handle_start_event,
            },

            invalid_handler: Handler {
                name: "Linefeed".to_string(),
                func: handle_start_event,
            },
        }
    }
}

fn handle_start_event<'a>(event: &u8, ctx: &'a mut InnerContext, state: &str) -> &'a Handler {
    match *event {
        XCL => {
            ctx.msg.encapsulation = true;
            &ctx.talker_handler
        }
        START => &ctx.talker_handler,
        _ => {
            ctx.error = format!(
                "Invalid event {} @{} in state {}",
                byte_2_print(event),
                ctx.event_count,
                state
            );
            &ctx.invalid_handler
        }
    }
}

fn handle_talker_event<'a>(event: &u8, ctx: &'a mut InnerContext, state: &str) -> &'a Handler {
    match *event {
        b'A'..=b'Z' => {
            ctx.chksum = ctx.chksum ^ event;
            ctx.collect.push(*event as char);
            if ctx.collect.len() > 1 {
                ctx.msg.talker = take(&mut ctx.collect);
                &ctx.msgtype_handler
            } else {
                &ctx.talker_handler
            }
        }
        _ => {
            ctx.error = format!(
                "Invalid event {} @{} in state {}",
                byte_2_print(event),
                ctx.event_count,
                state
            );
            &ctx.invalid_handler
        }
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
