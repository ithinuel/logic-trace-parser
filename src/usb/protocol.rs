use super::packet::{self, Packet};
use super::types::{Data, HandShake, Token};
use crate::pipeline::{self, Event as PipeEvent, EventData, EventIterator};
use anyhow::Result;

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
    T: Iterator<Item = PipeEvent>,
{
    type Item = PipeEvent;
    fn next(&mut self) -> Option<Self::Item> {
        let out: (_, Result<Box<dyn EventData>>) = loop {
            let (ts, event) = match self.it.next()? {
                (ts, Ok(ev)) => (ts, ev),
                (ts, Err(e)) => return Some((ts, Err(e))),
            };
            let event = *pipeline::downcast(event);
            match event {
                Packet::Reset => {
                    self.transaction_state = TransactionState::Idle;
                    break (ts, Ok(Box::new(Event::Reset)));
                }
                Packet::SoF(frm_num) => break (ts, Ok(Box::new(Event::Sof(frm_num)))),
                Packet::Token(token) => match self.transaction_state {
                    TransactionState::Idle => {
                        self.transaction_state = TransactionState::Token(token);
                    }
                    _ => break (ts, Err(anyhow::anyhow!("Unexpected token packet"))),
                },
                Packet::Data(data) => match self.transaction_state {
                    TransactionState::Token(token) => {
                        self.transaction_state = TransactionState::Data {
                            token,
                            data: Some(data),
                        };
                    }
                    _ => break (ts, Err(anyhow::anyhow!("Unexpected data packet"))),
                },
                Packet::HandShake(handshake) => {
                    let (token, data) = match self.transaction_state {
                        TransactionState::Token(token) => (token, None),
                        TransactionState::Data {
                            token,
                            ref mut data,
                        } => (token, data.take()),
                        _ => break (ts, Err(anyhow::anyhow!("Unexpected handshake packet"))),
                    };
                    self.transaction_state = TransactionState::Idle;
                    break (
                        ts,
                        Ok(Box::new(Event::Transaction(Transaction {
                            token,
                            data,
                            handshake,
                        }))),
                    );
                }
            }
        };

        Some(out)
    }
}

impl<T> ProtocolIterator<T>
where
    T: Iterator<Item = PipeEvent>,
{
    pub fn new(input: T) -> Self {
        Self {
            it: input,

            transaction_state: TransactionState::Idle,
        }
    }
}

impl<T: 'static + Iterator<Item = PipeEvent>> EventIterator for ProtocolIterator<T> {
    fn into_iterator(self: Box<Self>) -> Box<dyn Iterator<Item = PipeEvent>> {
        self
    }
    fn event_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Event>()
    }
    fn event_type_name(&self) -> &'static str {
        std::any::type_name::<Event>()
    }
}

pub fn build(pipeline: &mut Vec<Box<dyn EventIterator>>, args: &[String]) {
    use clap::{Arg, SubCommand};

    let _arg_matches = SubCommand::with_name("usb::protocol")
        .setting(clap::AppSettings::NoBinaryName)
        .arg(Arg::from_usage(
            "-v, --verbose verbose 'set to print events to stdout.'",
        ))
        .get_matches_from(args);

    if pipeline
        .last()
        .map(|node| node.event_type() != std::any::TypeId::of::<Packet>())
        .unwrap_or(false)
    {
        packet::build(pipeline, &[]);
    }

    match pipeline.pop() {
        None => panic!("Missing source for usb::protocol's parser"),
        Some(node) => {
            let it = node.into_iterator();
            let node = Box::new(ProtocolIterator::new(it));
            pipeline.push(node);
        }
    }
}
