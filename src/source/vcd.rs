use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{anyhow, Context};
use vcd::{Command, IdCode, Parser, TimescaleUnit, Value, VarType};

use super::Sample;
use crate::pipeline::{Event, EventData, EventIterator};

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
    type Item = Event;
    fn next(&mut self) -> Option<Self::Item> {
        if self.stopped {
            return None;
        }

        let out = loop {
            match self.input.next()? {
                Ok(cmd) => match cmd {
                    Command::Timescale(n, unit) => {
                        let unit = match unit {
                            TimescaleUnit::S => 1.,
                            TimescaleUnit::MS => 0.001,
                            TimescaleUnit::US => 0.000_001,
                            TimescaleUnit::NS => 0.000_000_001,
                            TimescaleUnit::PS => 0.000_000_000_001,
                            TimescaleUnit::FS => 0.000_000_000_000_001,
                        };
                        self.factor = (n as f64) * unit;
                    }
                    Command::Timestamp(ts) => {
                        let new_ts = (ts as f64) * self.factor;
                        if (self.first_ts + 0.1).abs() < f64::EPSILON {
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
                        break (
                            self.current_ts,
                            Ok(Box::new(Sample(self.state)) as Box<dyn EventData>),
                        );
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

impl<T: Read + 'static> EventIterator for VcdParser<T> {
    fn into_iterator(self: Box<Self>) -> Box<dyn Iterator<Item = Event>> {
        self
    }
    fn event_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<super::Sample>()
    }
    fn event_type_name(&self) -> &'static str {
        std::any::type_name::<super::Sample>()
    }
}

pub fn build(pipeline: &mut Vec<Box<dyn EventIterator>>, args: &[String]) {
    use clap::Arg;
    let _args = clap::SubCommand::with_name("logic2")
        .setting(clap::AppSettings::NoBinaryName)
        .args(&[Arg::with_name("file")
            .help("Input file. (may be a folder in case of Saleae Logic 2 exports.)")
            .required(true)])
        .get_matches_from(args);

    let file = std::fs::File::open(
        _args
            .value_of("file")
            .context("Fetching file argument")
            .unwrap(),
    )
    .context("Openning capture file.")
    .unwrap();
    let parser = Box::new(VcdParser::new(file));
    pipeline.push(parser);
}
