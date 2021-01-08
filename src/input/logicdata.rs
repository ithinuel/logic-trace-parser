use clap::{value_t, ArgMatches};
use nom::{
    do_parse, named_args,
    number::complete::{le_i64, le_u8},
};
use std::io::Read;

use super::Sample;

named_args!(
    parse_sample(freq: f64)<&[u8], (f64, Sample)>,
    do_parse!(
        ts: le_i64 >>
        smp: le_u8 >>
        ((ts as f64)/freq, Sample(smp.into()))
    )
);
pub struct LogicDataParser<T>
where
    T: Read,
{
    input: T,
    freq: f64,

    current_ts: f64,
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
        }
    }
}

impl<T> Iterator for LogicDataParser<T>
where
    T: Read,
{
    type Item = (f64, anyhow::Result<Sample>);
    fn next(&mut self) -> Option<Self::Item> {
        let mut buffer = [0; 9];

        if self.input.read_exact(&mut buffer).is_ok() {
            Some(match parse_sample(&buffer, self.freq) {
                Ok((_, (ts, sample))) => {
                    self.current_ts = ts;
                    (ts, Ok(sample))
                }
                Err(msg) => (self.current_ts, Err(anyhow::anyhow!("{:?}", msg))),
            })
        } else {
            None
        }
    }
}
