use super::protocol;
use crate::pipeline::{self, Event as PipeEvent, EventData, EventIterator};
use anyhow::{anyhow, Result};
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
impl From<ClassEvent> for InterfaceEvent {
    fn from(from: ClassEvent) -> InterfaceEvent {
        InterfaceEvent::Class(from)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceEvent {
    Class(ClassEvent),
}
impl From<InterfaceEvent> for DeviceEvent {
    fn from(from: InterfaceEvent) -> DeviceEvent {
        DeviceEvent::Interface(from)
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
    T: Iterator<Item = PipeEvent>,
{
    type Item = PipeEvent;
    fn next(&mut self) -> Option<Self::Item> {
        use protocol::Event;

        let out: (_, Result<Box<dyn EventData>>) = loop {
            let (ts, event) = match self.it.next()? {
                (ts, Ok(ev)) => (ts, ev),
                (ts, Err(e)) => break (ts, Err(e)),
            };
            let event = *pipeline::downcast(event);
            match event {
                Event::Sof(_) => continue,
                Event::Reset => break (ts, Ok(Box::new(DeviceEvent::Reset))),
                Event::Transaction(transaction) => {
                    let endpt = usize::from(transaction.token.endpoint);
                    let res = if endpt == 0 {
                        self.control.update(ts, transaction, &mut self.endpoints)
                    } else {
                        self.endpoints
                            .get_mut(&endpt)
                            .map(|endpoint| endpoint.update(ts, transaction))
                            .unwrap_or_else(|| Some(Err(anyhow!("Invalid endpoint {}", endpt))))
                    };
                    if let Some(res) = res {
                        let res = res.map(|v| Box::new(v) as Box<dyn EventData>);
                        break (ts, res);
                    }
                }
            }
        };
        Some(out)
    }
}

impl<T> DeviceEventIterator<T> {
    pub fn new(input: T) -> Self {
        Self {
            it: input,
            control: control::ControlEndpoint::new(),
            endpoints: HashMap::new(),
            _interfaces: (),
        }
    }
}

impl<T: 'static + Iterator<Item = PipeEvent>> EventIterator for DeviceEventIterator<T> {
    fn into_iterator(self: Box<Self>) -> Box<dyn Iterator<Item = PipeEvent>> {
        self
    }
    fn event_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<DeviceEvent>()
    }
    fn event_type_name(&self) -> &'static str {
        std::any::type_name::<DeviceEvent>()
    }
}

pub fn build(pipeline: &mut Vec<Box<dyn EventIterator>>, args: &[String]) {
    use clap::{Arg, SubCommand};
    let _arg_matches = SubCommand::with_name("usb::device")
        .setting(clap::AppSettings::NoBinaryName)
        .arg(Arg::from_usage(
            "-v, --verbose verbose 'set to print events to stdout.'",
        ))
        .get_matches_from(args);

    if pipeline
        .last()
        .map(|node| node.event_type() != std::any::TypeId::of::<protocol::Event>())
        .unwrap_or(false)
    {
        protocol::build(pipeline, &[]);
    }

    match pipeline.pop() {
        None => panic!("Missing source for usb::device's parser"),
        Some(node) => {
            let it = node.into_iterator();
            let node = Box::new(DeviceEventIterator::new(it));
            pipeline.push(node);
        }
    }
}
