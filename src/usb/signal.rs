use crate::input::Sample;
use clap::{value_t, App, Arg, ArgMatches, SubCommand};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
    SE0,
    J,
    K,
    SE1,
}

pub struct SignalIterator<T> {
    it: T,
    fs: bool,
    dp_mask: u64,
    dm_mask: u64,

    current_signal: Option<Signal>,
}

impl<T> Iterator for SignalIterator<T>
where
    T: Iterator<Item = (f64, anyhow::Result<Sample>)>,
{
    type Item = (f64, anyhow::Result<Signal>);
    fn next(&mut self) -> Option<Self::Item> {
        let out = loop {
            let (ts, smp) = self.it.next()?;
            let smp = match smp {
                Ok(Sample(smp)) => smp,
                Err(e) => break (ts, Err(e)),
            };

            let dp = (smp & self.dp_mask) == self.dp_mask;
            let dm = (smp & self.dm_mask) == self.dm_mask;

            let s = match (dp, dm, self.fs) {
                (true, true, _) => Signal::SE1,
                (true, false, true) | (false, true, false) => Signal::J,
                (true, false, false) | (false, true, true) => Signal::K,
                (false, false, _) => Signal::SE0,
            };
            if self
                .current_signal
                .map(|current| current != s)
                .unwrap_or(true)
            {
                self.current_signal = Some(s);
                break (ts, Ok(s));
            }
        };
        Some(out)
    }
}
impl<T> SignalIterator<T> {
    pub fn new(input: T, matches: &ArgMatches<'_>) -> Self {
        Self {
            it: input,
            fs: matches.is_present("fs"),
            dp_mask: 1 << value_t!(matches, "dp", u8).unwrap_or_else(|e| e.exit()),
            dm_mask: 1 << value_t!(matches, "dm", u8).unwrap_or_else(|e| e.exit()),
            current_signal: None,
        }
    }
}
pub trait SignalIteratorExt: Sized {
    fn into_signal(self, matches: &ArgMatches) -> SignalIterator<Self> {
        SignalIterator::new(self, matches)
    }
}
impl<T> SignalIteratorExt for T where T: Iterator<Item = (f64, anyhow::Result<Sample>)> {}

pub fn args() -> [Arg<'static, 'static>; 3] {
    [
        Arg::from_usage("--dp [dp] 'Channel used for the d+ pin'").default_value("0"),
        Arg::from_usage("--dm [dm] 'Channel used for the d- pin'").default_value("1"),
        Arg::from_usage("--fs 'Indicates that the device is full-speed USB'"),
    ]
}

pub fn subcommand() -> App<'static, 'static> {
    SubCommand::with_name("usb::signal").args(&args())
}
