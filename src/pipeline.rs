use std::any::Any;
use std::fmt::Debug;

use anyhow::Result;
use colored::*;

pub trait EventData: Debug + Any {
    fn as_debug(&self) -> &dyn Debug;
    fn into_debug(self: Box<Self>) -> Box<dyn Debug>;
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}
impl<T: Debug + Any> EventData for T {
    fn as_debug(&self) -> &dyn Debug {
        self
    }
    fn into_debug(self: Box<Self>) -> Box<dyn Debug> {
        self
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

pub fn downcast<T: 'static>(event: Box<dyn EventData>) -> Box<T> {
    let name = event.type_name();
    let any = event.into_any();
    any.downcast::<T>().unwrap_or_else(|_| {
        eprintln!(
            "{} Unexpected event type {} while expecting {}",
            "Error".red().bold(),
            name,
            std::any::type_name::<T>()
        );
        std::process::exit(1);
    })
}
pub fn downcast_ref<T: 'static>(event: &dyn EventData) -> &T {
    let name = event.type_name();
    event.as_any().downcast_ref().unwrap_or_else(|| {
        eprintln!(
            "{}: Unexpected event type {} while expecting {}",
            "Error".red().bold(),
            name,
            std::any::type_name::<T>()
        );
        std::process::exit(1);
    })
}

pub type Event = (f64, Result<Box<dyn EventData>>);

pub trait EventIterator: Iterator<Item = Event> {
    fn into_iterator(self: Box<Self>) -> Box<dyn Iterator<Item = Event>>;
    fn event_type(&self) -> std::any::TypeId;
    fn event_type_name(&self) -> &'static str;
}
