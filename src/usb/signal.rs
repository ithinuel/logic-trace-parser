use clap::{value_t, ArgMatches};

use crate::pipeline::{self, Event, EventIterator};
use crate::source::Sample;

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
    verbose: bool,
}

impl<T> Iterator for SignalIterator<T>
where
    T: Iterator<Item = Event>,
{
    type Item = Event;
    fn next(&mut self) -> Option<Self::Item> {
        let res: Event = loop {
            let (ts, event) = match self.it.next()? {
                (ts, Ok(ev)) => (ts, ev),
                (ts, Err(e)) => break (ts, Err(e)),
            };

            let smp = pipeline::downcast::<Sample>(event).0;

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
                if self.verbose {}
                break (ts, Ok(Box::new(s)));
            }
        };
        if self.verbose {
            println!("{:.9}: {:?}", res.0, res.1);
        }
        Some(res)
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
            verbose: matches.is_present("verbose"),
        }
    }
}
impl<T: 'static + Iterator<Item = Event>> EventIterator for SignalIterator<T> {
    fn into_iterator(self: Box<Self>) -> Box<dyn Iterator<Item = Event>> {
        self
    }
    fn event_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Signal>()
    }
    fn event_type_name(&self) -> &'static str {
        std::any::type_name::<Signal>()
    }
}

pub fn build(pipeline: &mut Vec<Box<dyn EventIterator>>, args: &[String]) {
    use clap::{Arg, SubCommand};
    let arg_matches = SubCommand::with_name("usb::signal")
        .setting(clap::AppSettings::NoBinaryName)
        .args(&[
            Arg::from_usage("-v, --verbose verbose 'set to print events to stdout.'"),
            Arg::from_usage("--dp [dp] 'Channel used for the d+ pin'").default_value("0"),
            Arg::from_usage("--dm [dm] 'Channel used for the d- pin'").default_value("1"),
            Arg::from_usage("--fs 'Indicates that the device is full-speed USB'"),
        ])
        .get_matches_from(args);

    if let Some(node) = pipeline.last() {
        if node.event_type() != std::any::TypeId::of::<Sample>() {
            panic!(
                "Invalid input type. Exected {} but got {}",
                std::any::type_name::<Sample>(),
                node.event_type_name()
            )
        }
    }

    match pipeline.pop() {
        None => panic!("Missing source for usb::signal's parser"),
        Some(node) => {
            let it = node.into_iterator();
            let node = Box::new(SignalIterator::new(it, &arg_matches));
            pipeline.push(node);
        }
    }
}
