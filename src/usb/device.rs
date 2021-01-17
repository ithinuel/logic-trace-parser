use super::protocol;
use anyhow::anyhow;
use clap::{App, Arg, ArgMatches, SubCommand};
use std::collections::HashMap;

mod cdc;
mod msd;

mod lang_id;
mod types;

mod control;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ClassEvent {
    CdC(cdc::Event),
    MassStorage(msd::Event),
}
impl Into<InterfaceEvent> for ClassEvent {
    fn into(self) -> InterfaceEvent {
        InterfaceEvent::Class(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceEvent {
    Class(ClassEvent),
}
impl Into<DeviceEvent> for InterfaceEvent {
    fn into(self) -> DeviceEvent {
        DeviceEvent::Interface(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceEvent {
    Reset,
    Control(control::Event),
    Interface(InterfaceEvent),
}

trait Endpoint {
    fn update(
        &mut self,
        timestamp: f64,
        transaction: protocol::Transaction,
    ) -> Option<anyhow::Result<DeviceEvent>>;
}
pub struct DeviceEventIterator<T> {
    it: T,

    control: control::ControlEndpoint,
    endpoints: HashMap<usize, Box<dyn Endpoint>>,
    _interfaces: (),
    // classes
}

impl<T> Iterator for DeviceEventIterator<T>
where
    T: Iterator<Item = (f64, anyhow::Result<protocol::Event>)>,
{
    type Item = (f64, anyhow::Result<DeviceEvent>);
    fn next(&mut self) -> Option<Self::Item> {
        let out = loop {
            match self.it.next()? {
                (_, Ok(protocol::Event::Sof(_))) => {}
                (ts, Ok(protocol::Event::Reset)) => break (ts, Ok(DeviceEvent::Reset)),
                (ts, Ok(protocol::Event::Transaction(transaction))) => {
                    let endpt = usize::from(transaction.token.endpoint);
                    match if endpt == 0 {
                        self.control.update(ts, transaction, &mut self.endpoints)
                    } else {
                        self.endpoints
                            .get_mut(&endpt)
                            .map(|endpoint| endpoint.update(ts, transaction))
                            .unwrap_or_else(|| Some(Err(anyhow!("Invalid endpoint {}", endpt))))
                    } {
                        Some(res) => break (ts, res),
                        None => {}
                    }
                }
                (ts, Err(e)) => break (ts, Err(e)),
            }
        };
        Some(out)
    }
}

impl<T> DeviceEventIterator<T> {
    pub fn new<'a>(input: T, _matches: &ArgMatches<'a>) -> Self {
        Self {
            it: input,
            control: control::ControlEndpoint::new(),
            endpoints: HashMap::new(),
            _interfaces: (),
        }
    }
}
pub trait DeviceEventIteratorExt: Sized {
    fn into_device(self, matches: &ArgMatches) -> DeviceEventIterator<Self> {
        DeviceEventIterator::new(self, matches)
    }
}
impl<T> DeviceEventIteratorExt for T where T: Iterator<Item = (f64, anyhow::Result<protocol::Event>)>
{}

pub fn args() -> [Arg<'static, 'static>; 3] {
    protocol::args()
}
pub fn subcommand() -> App<'static, 'static> {
    SubCommand::with_name("usb::device").args(&args())
}
