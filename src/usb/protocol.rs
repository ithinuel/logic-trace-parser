use super::packet::Packet;
use super::types::{Data, HandShake, Token};
use clap::{App, Arg, ArgMatches, SubCommand};

#[derive(Debug)]
pub enum Event {
    Reset,
    Sof(u16),
    Transaction(Transaction),
}

#[derive(PartialEq, Debug)]
pub enum TransactionState {
    Idle,
    Token(Token),
    Data { token: Token, data: Option<Data> },
}

#[derive(Debug)]
pub struct Transaction {
    pub token: Token,
    pub data: Option<Data>,
    pub handshake: HandShake,
}

pub struct ProtocolIterator<T> {
    it: T,
    transaction_state: TransactionState,
}
impl<T> Iterator for ProtocolIterator<T>
where
    T: Iterator<Item = (f64, anyhow::Result<Packet>)>,
{
    type Item = (f64, anyhow::Result<Event>);
    fn next(&mut self) -> Option<Self::Item> {
        let out = loop {
            match self.it.next()? {
                (ts, Ok(Packet::Reset)) => {
                    self.transaction_state = TransactionState::Idle;
                    break (ts, Ok(Event::Reset));
                }
                (ts, Ok(Packet::SoF(frm_num))) => break (ts, Ok(Event::Sof(frm_num))),
                (ts, Ok(Packet::Token(token))) => match self.transaction_state {
                    TransactionState::Idle => {
                        self.transaction_state = TransactionState::Token(token);
                    }
                    _ => break (ts, Err(anyhow::anyhow!("Unexpected token packet"))),
                },
                (ts, Ok(Packet::Data(data))) => match self.transaction_state {
                    TransactionState::Token(token) => {
                        self.transaction_state = TransactionState::Data {
                            token,
                            data: Some(data),
                        };
                    }
                    _ => break (ts, Err(anyhow::anyhow!("Unexpected data packet"))),
                },
                (ts, Ok(Packet::HandShake(handshake))) => {
                    let (token, data) = match self.transaction_state {
                        TransactionState::Token(token) => (token, None),
                        TransactionState::Data {
                            token,
                            ref mut data,
                        } => (token, std::mem::replace(data, None)),
                        _ => break (ts, Err(anyhow::anyhow!("Unexpected handshake packet"))),
                    };
                    self.transaction_state = TransactionState::Idle;
                    break (
                        ts,
                        Ok(Event::Transaction(Transaction {
                            token,
                            data,
                            handshake,
                        })),
                    );
                }

                (ts, Err(e)) => break (ts, Err(e)),
            }
        };

        Some(out)
    }
}

impl<T> ProtocolIterator<T>
where
    T: Iterator<Item = (f64, anyhow::Result<Packet>)>,
{
    pub fn new<'a>(input: T, _matches: &ArgMatches<'a>) -> Self {
        Self {
            it: input,

            transaction_state: TransactionState::Idle,
        }
    }
}
pub trait ProtocolIteratorExt: Iterator<Item = (f64, anyhow::Result<Packet>)> + Sized {
    fn into_protocol(self, matches: &ArgMatches) -> ProtocolIterator<Self> {
        ProtocolIterator::new(self, matches)
    }
}
impl<T> ProtocolIteratorExt for T where T: Iterator<Item = (f64, anyhow::Result<Packet>)> {}

pub fn args() -> [Arg<'static, 'static>; 3] {
    crate::usb::packet::args()
}
pub fn subcommand() -> App<'static, 'static> {
    SubCommand::with_name("usb::protocol").args(&args())
}
