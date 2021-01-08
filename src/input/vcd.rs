use super::Sample;
use anyhow::anyhow;
use std::collections::BTreeMap;
use std::io::Read;

use vcd::{Command, IdCode, Parser, TimescaleUnit, Value, VarType};

pub struct VcdParser<T>
where
    T: Read,
{
    input: Parser<T>,
    factor: f64,
    first_ts: f64,
    current_ts: f64,
    vars: BTreeMap<IdCode, usize>,
    state: u64,
    stopped: bool,
}

impl<T> VcdParser<T>
where
    T: Read,
{
    pub fn new(input: T) -> Self {
        Self {
            input: Parser::new(input),
            factor: 1.,
            first_ts: -0.1, // pre-trigger buffer size
            current_ts: -0.1,
            vars: BTreeMap::new(),
            state: 0,
            stopped: false,
        }
    }
}

impl<T> Iterator for VcdParser<T>
where
    T: Read,
{
    type Item = (f64, anyhow::Result<Sample>);
    fn next(&mut self) -> Option<Self::Item> {
        if self.stopped {
            return None;
        }

        let out = loop {
            match self.input.next()? {
                Ok(cmd) => match cmd {
                    Command::Timescale(n, unit) => {
                        self.factor = (n as f64)
                            * match unit {
                                TimescaleUnit::S => 1.,
                                TimescaleUnit::MS => 0.001,
                                TimescaleUnit::US => 0.000001,
                                TimescaleUnit::NS => 0.000000001,
                                TimescaleUnit::PS => 0.000000000001,
                                TimescaleUnit::FS => 0.000000000000001,
                            };
                    }
                    Command::Timestamp(ts) => {
                        let new_ts = (ts as f64) * self.factor;
                        if self.first_ts == -0.1 {
                            self.first_ts = new_ts;
                        }

                        let new_ts = new_ts - self.first_ts - 0.1;
                        if self.current_ts > new_ts {
                            self.stopped = true;
                            break (self.current_ts, Err(anyhow!("Timestamp must be monotonic")));
                        }
                        self.current_ts = new_ts;
                    }
                    Command::ChangeScalar(id, v) => {
                        let v = match v {
                            Value::V0 => 0,
                            Value::V1 => 1,
                            _ => {
                                self.stopped = true;
                                break (
                                    self.current_ts,
                                    Err(anyhow!("Unsupported value : {:?}", v)),
                                );
                            }
                        };
                        let shift = self.vars[&id];
                        self.state &= !(1 << shift);
                        self.state |= v << shift;
                        break (self.current_ts, Ok(Sample(self.state)));
                    }
                    Command::VarDef(ty, _sz, id, name) => {
                        if ty == VarType::Wire {
                            self.vars.insert(
                                id,
                                name.split('_').nth(1).unwrap().parse::<usize>().unwrap(),
                            );
                        } else {
                            break (
                                self.current_ts,
                                Err(anyhow!("Unsupported VarType: {:?}", ty)),
                            );
                        }
                    }
                    _v => {
                        //eprintln!("ignoring: {:?}", v);
                    }
                },
                Err(err) => break (self.current_ts, Err(anyhow!("{:?}", err))),
            }
        };
        Some(out)
    }
}
