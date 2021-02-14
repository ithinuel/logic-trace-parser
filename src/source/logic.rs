use std::{convert::TryInto, io::Read};

use anyhow::Context;
use clap::{value_t, ArgMatches};

use super::Sample;
use crate::pipeline::{Event, EventIterator};

pub struct LogicDataParser<T>
where
    T: Read,
{
    input: T,
    freq: f64,

    current_ts: f64,
    stopped: bool,
}

impl<T> LogicDataParser<T>
where
    T: Read,
{
    pub fn new(input: T, matches: &ArgMatches<'_>) -> Self {
        let mut freq = value_t!(matches, "freq", f64).unwrap_or_else(|e| e.exit());
        if freq == 0. {
            freq = 1.;
        }
        Self {
            input,
            freq,
            current_ts: 0.,
            stopped: false,
        }
    }
}

impl<T> Iterator for LogicDataParser<T>
where
    T: Read,
{
    type Item = Event;
    fn next(&mut self) -> Option<Self::Item> {
        if self.stopped {
            return None;
        }

        let mut buffer = [0; 8];

        let ts = match self.input.read_exact(&mut buffer) {
            Ok(_) => {
                let ts =
                    i64::from_le_bytes(buffer[..8].try_into().unwrap_or_else(|_| unreachable!()));
                let ts = ts as f64; // lossy conversion from i64 to f64;
                ts / self.freq
            }
            Err(e) => return Some((self.current_ts, Err(e.into()))),
        };
        let smp = match self.input.read_exact(&mut buffer[..1]) {
            Ok(_) => Sample(buffer[0].into()),
            Err(e) => return Some((self.current_ts, Err(e.into()))),
        };

        self.current_ts = ts;
        Some((ts, Ok(Box::new(smp))))
    }
}

impl<T: Read + 'static> EventIterator for LogicDataParser<T> {
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

pub fn build(pipeline: &mut Vec<Box<dyn EventIterator>>, args: &Vec<String>) {
    use clap::Arg;
    let args = clap::SubCommand::with_name("logic2")
        .setting(clap::AppSettings::NoBinaryName)
        .args(&[
            Arg::from_usage("-f, --freq [freq] 'Sample frequency (only used on binary input)'")
                .default_value("1."),
            Arg::with_name("file")
                .help("Input file. (may be a folder in case of Saleae Logic 2 exports.)")
                .required(true),
        ])
        .get_matches_from(args);

    let file = std::fs::File::open(
        args.value_of("file")
            .context("Fetching file argument")
            .unwrap(),
    )
    .context("Openning capture file.")
    .unwrap();
    let parser = Box::new(LogicDataParser::new(file, &args));
    pipeline.push(parser);
}
