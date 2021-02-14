use std::collections::VecDeque;

use anyhow::{anyhow, Result};
use colored::Colorize;
use itertools::{peek_nth, PeekNth};

use super::signal::{self, Signal};
use crate::pipeline::{self, Event, EventData, EventIterator};

#[derive(Debug, Clone, Copy)]
pub enum Byte {
    Reset,
    Idle,
    Byte(u8),
    Eop,
}
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum State {
    Reset,
    Idle,
    EopStart,
    Receiving,
    Suspended,
}

pub struct ByteIterator<T: Iterator> {
    it: PeekNth<T>,

    bit_len: f64,

    state: State,
    counter: u16,
    shift_reg: u16,
    consecutive_ones: u8,

    ev_queue: VecDeque<(f64, Result<Box<dyn EventData>>)>,
    verbose: bool,
}

impl<T: Iterator> ByteIterator<T> {
    fn push_bits(&mut self, ulen: u64) {
        let consecutive_ones = ulen - 1;
        let bits = if self.consecutive_ones == 6 {
            // account for bit stuffing
            ulen - 1
        } else {
            ulen
        } as u16;

        self.counter += bits;
        self.shift_reg >>= bits;
        if consecutive_ones != 0 {
            let mask = (1 << consecutive_ones) - 1;
            self.shift_reg |= mask << (16 - consecutive_ones);
        }
        //println!("{:016b}", self.shift_reg);
        self.consecutive_ones = (ulen - 1) as u8;
    }
}

impl<T> Iterator for ByteIterator<T>
where
    T: Iterator<Item = Event>,
{
    type Item = Event;
    fn next(&mut self) -> Option<Self::Item> {
        while self.ev_queue.is_empty() {
            // cover for cases were DP & DP are slightly de-synchronized and generate spurious SE0
            // & SE1.
            let (t0, event) = match self.it.next()? {
                (ts, Ok(ev)) => (ts, ev),
                (ts, Err(e)) => return Some((ts, Err(e))),
            };
            let sig0 = *pipeline::downcast(event);

            let bit_len = self.bit_len;

            //  TODO: coerse following signals that are either:
            // same signal,
            // SE0 or SE1 of less than half a bit duration.

            let next_ts = loop {
                let (t1, duration, sig1) = match self.it.peek() {
                    Some((t1, Ok(sig1))) => {
                        let t1 = *t1;
                        let sig1 = *pipeline::downcast_ref::<Signal>(sig1);
                        match self.it.peek_nth(1) {
                            Some((t2, _)) => (t1, t2 - t1, sig1),
                            _ => break None,
                        }
                    }
                    _ => break None,
                };

                if !(sig0 == sig1
                    || ((sig1 == Signal::SE0 || sig1 == Signal::SE1) && duration < (bit_len / 2.)))
                {
                    break Some(t1);
                }
                self.it.next();
            }
            .unwrap_or(f64::INFINITY);

            let ulen = ((next_ts - t0) / self.bit_len).round() as u64;
            let len = next_ts - t0;
            let nts = next_ts;

            if sig0 == Signal::SE1 {
                self.ev_queue
                    .push_back((t0, Err(anyhow!("Unexpected bus state"))));
            } else if sig0 == Signal::SE0 && len > 0.010 {
                self.ev_queue.push_back((t0, Ok(Box::new(Byte::Reset))));
                self.state = State::Reset;
                self.counter = 0;
            } else {
                //println!("{:?} {:?} {:?} {} {}", self.state, current, next, ulen, len);
                match self.state {
                    State::Reset => {
                        // we only expect a J
                        self.ev_queue.push_back(if sig0 == Signal::J {
                            (t0, Ok(Box::new(Byte::Idle)))
                        } else {
                            (t0, Err(anyhow!("Unexpected bus state after Reset")))
                        });
                        self.state = State::Idle;
                    }
                    State::Idle => match sig0 {
                        Signal::K => {
                            if ulen >= 7 {
                                self.state = State::Suspended;
                            } else {
                                self.state = State::Receiving;
                                self.push_bits(ulen);
                            }
                        }
                        Signal::J => {}
                        Signal::SE0 => {}
                        Signal::SE1 => unreachable!(),
                    },
                    State::Receiving => {
                        if sig0 == Signal::SE0 && ulen == 2 {
                            assert_eq!(self.counter, 0);
                            self.state = State::EopStart;
                        } else if ulen <= 7 && (sig0 == Signal::K || sig0 == Signal::J) {
                            self.push_bits(ulen);
                        } else {
                            // framing error
                            self.state = State::Idle;
                            self.ev_queue.push_back((t0, Err(anyhow!("Framing Error"))));
                        }
                    }
                    State::EopStart => {
                        // we only expect J with J.len >= 1bit
                        if sig0 == Signal::J && ulen >= 1 {
                            self.ev_queue
                                .push_back((t0 - 2. * self.bit_len, Ok(Box::new(Byte::Eop))));
                            self.state = State::Idle;
                            if ulen > 1 {
                                self.ev_queue
                                    .push_back((t0 + self.bit_len, Ok(Box::new(Byte::Idle))));
                            }
                        } else {
                            self.state = State::Idle;
                            self.ev_queue.push_back((
                                t0,
                                Err(anyhow!("Unexpected bus state after start of End of Packet")),
                            ));
                        }
                    }
                    State::Suspended => {
                        // we only expect SE0 with SE0.len == 2
                        if sig0 == Signal::SE0 && ulen == 2 {
                            self.state = State::EopStart;
                        } else {
                            self.state = State::Idle;
                            self.ev_queue.push_back((
                                t0,
                                Err(anyhow!("Unexpected bus state after suspended state.")),
                            ));
                        }
                    }
                }
            }
            if self.counter >= 8 {
                self.ev_queue.push_back((
                    nts,
                    Ok(Box::new(Byte::Byte(
                        ((self.shift_reg >> (16 - self.counter)) & 0xFF) as u8,
                    ))),
                ));
                self.counter -= 8;
            }
        }
        self.ev_queue.pop_front().map(|ev| {
            if self.verbose {
                println!("{:10.9}: {}: {:?}", ev.0, "Byte".green().bold(), ev.1);
            }
            ev
        })
    }
}

impl<T: Iterator> ByteIterator<T> {
    pub fn new<'a>(input: T, matches: &clap::ArgMatches<'a>) -> Self {
        Self {
            it: peek_nth(input),
            bit_len: 1.
                / if matches.is_present("fs") {
                    12_000_000.
                } else {
                    1_500_000.
                },
            state: State::Idle,
            counter: 0,
            shift_reg: 0,
            consecutive_ones: 0,
            ev_queue: VecDeque::new(),
            verbose: matches.is_present("-v"),
        }
    }
}

impl<T: 'static + Iterator<Item = Event>> EventIterator for ByteIterator<T> {
    fn into_iterator(self: Box<Self>) -> Box<dyn Iterator<Item = Event>> {
        self
    }
    fn event_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Byte>()
    }
    fn event_type_name(&self) -> &'static str {
        std::any::type_name::<Byte>()
    }
}

pub fn build(pipeline: &mut Vec<Box<dyn EventIterator>>, args: &[String]) {
    use clap::{Arg, SubCommand};
    let arg_matches = SubCommand::with_name("usb::byte")
        .setting(clap::AppSettings::NoBinaryName)
        .args(&[
            Arg::from_usage("-v, --verbose verbose 'set to print events to stdout.'"),
            Arg::from_usage("--fs 'the Usb interface is full speed'"),
        ])
        .get_matches_from(args);

    if pipeline
        .last()
        .map(|node| node.event_type() != std::any::TypeId::of::<Signal>())
        .unwrap_or(false)
    {
        signal::build(pipeline, &[]);
    }

    match pipeline.pop() {
        None => panic!("Missing source for usb::device's parser"),
        Some(node) => {
            let it = node.into_iterator();
            let node = Box::new(ByteIterator::new(it, &arg_matches));
            pipeline.push(node);
        }
    }
}
