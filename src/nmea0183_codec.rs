use crate::nmea0183_codec::context::Context;
use crate::Nmea0183Msg;
use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

mod context;

pub struct Nmea0183Codec {
    ctx: Context,
    first: bool,
}

impl Default for Nmea0183Codec {
    fn default() -> Self {
        Self {
            ctx: Context::new(),
            first: true,
        }
    }
}

impl Decoder for Nmea0183Codec {
    type Item = Nmea0183Msg;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut rc = Ok(None);
        // eprintln!("decode({}, offset: {})", to_string(src), offset);
        loop {
            let offset = self.ctx.get_event_count();
            let position =
                src[offset..]
                    .as_ref()
                    .iter()
                    .position(|b| match self.ctx.handle_event(b) {
                        Ok(result) => {
                            if let Some(result) = result {
                                rc = Ok(Some(result));
                                if self.first == true {
                                    self.first = false
                                }
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

            if self.first == true {
                eprintln!("ignoring first error: {:?}", rc);
                self.first = false
            } else {
                break;
            }
        }
        rc
    }
}

impl Encoder<String> for Nmea0183Codec {
    type Error = std::io::Error;
    fn encode(&mut self, _item: String, _dst: &mut BytesMut) -> Result<(), Self::Error> {
        Ok(())
    }
}
