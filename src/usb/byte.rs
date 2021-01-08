use super::signal::Signal;
use clap::{App, Arg, ArgMatches, SubCommand};
use std::collections::VecDeque;

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

pub struct ByteIterator<T> {
    it: T,
    look_ahead: Option<(f64, Signal)>,

    bit_len: f64,

    state: State,
    counter: u16,
    shift_reg: u16,
    consecutive_ones: u8,

    ev_queue: VecDeque<(f64, anyhow::Result<Byte>)>,
}

impl<T> ByteIterator<T> {
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

macro_rules! opt_bail {
    ($input:expr) => {
        match $input? {
            (ts, Ok(smp)) => (ts, smp),
            (ts, Err(e)) => return Some((ts, Err(e))),
        }
    };
}

impl<T> Iterator for ByteIterator<T>
where
    T: Iterator<Item = (f64, anyhow::Result<Signal>)>,
{
    type Item = (f64, anyhow::Result<Byte>);
    fn next(&mut self) -> Option<Self::Item> {
        while self.ev_queue.is_empty() {
            // cover for cases were DP & DP are slightly de-synchronized and generate spurious SE0
            // & SE1.
            let mut spurious_signal_duration = 0.;
            let mut current = match self.look_ahead.take() {
                Some(current) => current,
                None => opt_bail!(self.it.next()),
            };

            let mut next = opt_bail!(self.it.next());

            if (current.1 == Signal::SE0 || current.1 == Signal::SE1)
                && (next.0 - current.0) <= (self.bit_len / 2.)
            {
                spurious_signal_duration = next.0 - current.0;
                current = next;
                next = opt_bail!(self.it.next());
            }
            self.look_ahead = Some(next);

            /*println!(
                "{:9.9} {:.0}—{}: {:?} ",
                current.0,
                (next.0 - current.0) * 1_000_000_000.,
                ulen,
                current.1,
            );*/

            let (ts, sig) = current;
            let ulen = ((next.0 - ts + spurious_signal_duration) / self.bit_len).round() as u64;
            let len = next.0 - ts;
            let nts = next.0;

            if sig == Signal::SE1 {
                self.ev_queue
                    .push_back((ts, Err(anyhow::anyhow!("Unexpected bus state"))));
            } else if sig == Signal::SE0 && len > 0.020 {
                self.ev_queue.push_back((ts, Ok(Byte::Reset)));
                self.state = State::Reset;
                self.counter = 0;
            } else {
                //println!("{:?} {:?} {:?} {} {}", self.state, current, next, ulen, len);
                match self.state {
                    State::Reset => {
                        // we only expect a J
                        self.ev_queue.push_back(if sig == Signal::J {
                            (ts, Ok(Byte::Idle))
                        } else {
                            (ts, Err(anyhow::anyhow!("Unexpected bus state after Reset")))
                        });
                        self.state = State::Idle;
                    }
                    State::Idle => match sig {
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
                        if sig == Signal::SE0 && ulen == 2 {
                            assert_eq!(self.counter, 0);
                            self.state = State::EopStart;
                        } else if ulen <= 7 && (sig == Signal::K || sig == Signal::J) {
                            self.push_bits(ulen);
                        } else {
                            // framing error
                            self.state = State::Idle;
                            self.ev_queue
                                .push_back((ts, Err(anyhow::anyhow!("Framing Error"))));
                        }
                    }
                    State::EopStart => {
                        // we only expect J with J.len >= 1bit
                        if sig == Signal::J && ulen >= 1 {
                            self.ev_queue
                                .push_back((ts - 2. * self.bit_len, Ok(Byte::Eop)));
                            self.state = State::Idle;
                            if ulen > 1 {
                                self.ev_queue.push_back((ts + self.bit_len, Ok(Byte::Idle)));
                            }
                        } else {
                            self.state = State::Idle;
                            self.ev_queue.push_back((
                                ts,
                                Err(anyhow::anyhow!(
                                    "Unexpected bus state after start of End of Packet"
                                )),
                            ));
                        }
                    }
                    State::Suspended => {
                        // we only expect SE0 with SE0.len == 2
                        if sig == Signal::SE0 && ulen == 2 {
                            self.state = State::EopStart;
                        } else {
                            self.state = State::Idle;
                            self.ev_queue.push_back((
                                ts,
                                Err(anyhow::anyhow!(
                                    "Unexpected bus state after suspended state."
                                )),
                            ));
                        }
                    }
                }
            }
            if self.counter >= 8 {
                self.ev_queue.push_back((
                    nts,
                    Ok(Byte::Byte(
                        ((self.shift_reg >> (16 - self.counter)) & 0xFF) as u8,
                    )),
                ));
                self.counter -= 8;
            }
        }
        self.ev_queue.pop_front()
    }
}

impl<T> ByteIterator<T> {
    pub fn new<'a>(input: T, matches: &ArgMatches<'a>) -> Self {
        Self {
            it: input,
            look_ahead: None,
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
        }
    }
}
pub trait ByteIteratorExt: Sized {
    fn into_byte(self, matches: &ArgMatches) -> ByteIterator<Self> {
        ByteIterator::new(self, matches)
    }
}
impl<T> ByteIteratorExt for T where T: Iterator<Item = (f64, anyhow::Result<Signal>)> {}

pub fn args() -> [Arg<'static, 'static>; 3] {
    crate::usb::signal::args()
}
pub fn subcommand() -> App<'static, 'static> {
    SubCommand::with_name("usb::byte").args(&args())
}
