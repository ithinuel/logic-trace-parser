use crate::serial::{self, SerialEvent};
use clap::{App, ArgMatches, SubCommand};
use std::net::Ipv4Addr;

#[derive(Debug)]
pub enum WizFi310Event {
    Command(String),
    Sent(String),
    Recv(RecvHeader, String),
    Resp(String),
}
#[derive(Debug)]
pub struct RecvHeader {
    socket_id: u8,
    ip: Ipv4Addr,
    port: u16,
}

pub struct Wizfi310<T> {
    it: T,
    data_to_send: usize,
    data_to_receive: usize,
    recv_header: Option<RecvHeader>,
    // sockets ?
    tx: String,
    rx: String,
}

impl<T> Iterator for Wizfi310<T>
where
    T: Iterator<Item = (f64, anyhow::Result<SerialEvent>)>,
{
    type Item = (f64, anyhow::Result<WizFi310Event>);

    fn next(&mut self) -> Option<Self::Item> {
        let out = loop {
            let (ts, ev) = match self.it.next()? {
                (ts, Ok(ev)) => (ts, ev),
                (ts, Err(e)) => return Some((ts, Err(e))),
            };
            match ev {
                SerialEvent::Tx(c) => {
                    self.tx.push(c as char);

                    if self.data_to_send != 0 {
                        if self.data_to_send == self.tx.chars().count() {
                            self.data_to_send = 0;

                            let mut v = String::new();
                            std::mem::swap(&mut v, &mut self.tx);
                            break (ts, Ok(WizFi310Event::Sent(v)));
                        }
                    } else if (c as char) == '\r' {
                        let mut v = String::new();
                        std::mem::swap(&mut v, &mut self.tx);
                        break (ts, Ok(WizFi310Event::Command(v)));
                    }
                }
                SerialEvent::Rx(c) => {
                    self.rx.push(c as char);

                    if self.data_to_receive != 0 {
                        if self.data_to_receive == self.rx.chars().count() {
                            self.data_to_receive = 0;

                            let mut v = String::new();
                            std::mem::swap(&mut v, &mut self.rx);
                            break (
                                ts,
                                Ok(WizFi310Event::Recv(self.recv_header.take().unwrap(), v)),
                            );
                        }
                    } else if (c as char) == '\n' {
                        if self.rx.starts_with("[") && self.rx.ends_with("]\r\n") {
                            if self.rx.contains(",") {
                                let line: String =
                                    self.rx.chars().skip(1).take(self.rx.len() - 4).collect();
                                self.data_to_send =
                                    line.split(',').last().and_then(|v| v.parse().ok()).unwrap()
                            }
                        }
                        let mut v = String::new();
                        std::mem::swap(&mut v, &mut self.rx);
                        break (ts, Ok(WizFi310Event::Resp(v)));
                    } else if (c as char) == '}' {
                        let header = self
                            .rx
                            .chars()
                            .skip(1)
                            .take(self.rx.len() - 2)
                            .collect::<String>();
                        let mut hsplit = header.split(',');
                        let event = RecvHeader {
                            socket_id: hsplit.next().and_then(|v| v.parse().ok()).unwrap(),
                            ip: hsplit.next().and_then(|v| v.parse().ok()).unwrap(),
                            port: hsplit.next().and_then(|v| v.parse().ok()).unwrap(),
                        };
                        self.data_to_receive = hsplit.next().and_then(|v| v.parse().ok()).unwrap();
                        self.recv_header = Some(event);
                        self.rx.clear();
                    }
                }
                _ => {}
            }
            // push byte to appropriate buffer
            // check buffer for completion
            // if rx in data mode:
            //      has buf len reached expected length ?
            //
        };
        Some(out)
    }
}

impl<T> Wizfi310<T> {
    pub fn new<'a>(input: T, _matches: &ArgMatches<'a>) -> Wizfi310<T> {
        Self {
            it: input,
            data_to_send: 0,
            data_to_receive: 0,
            recv_header: None,
            tx: String::new(),
            rx: String::new(),
        }
    }
}
pub trait Wizfi310IteratorExt: Sized {
    fn into_wizfi310(self, matches: &ArgMatches) -> Wizfi310<Self> {
        Wizfi310::new(self, matches)
    }
}
impl<T> Wizfi310IteratorExt for T where T: Iterator<Item = (f64, anyhow::Result<SerialEvent>)> {}

pub fn subcommand() -> App<'static, 'static> {
    SubCommand::with_name("wizfi310").args(&serial::args())
}
