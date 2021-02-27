use itertools::Itertools;
use std::convert::TryFrom;

use crate::usb::protocol::Transaction;
use crate::usb::types::HandShake;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Rx(Vec<u8>),
    Tx(Vec<u8>),
}

impl From<Event> for super::ClassEvent {
    fn from(event: Event) -> super::ClassEvent {
        super::ClassEvent::CdC(event)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceSubClass {
    Reserved0,
    DirectLineControlModel,
    AbstractControlModel,
    TelephoneControlModel,
    MultiChannelControlModel,
    CAPIControlModel,
    EthernetNetworkingControlModel,
    ATMNetworkingControlModel,
    WirelessHandsetControlModel,
    DeviceManagement,
    MobileDirectLineModel,
    OBEX,
    EthernetEmulationModel,
    NetworkControlModel,
    ReservedForFutureUse(u8),
    VendorSpecific(u8),
    Unkown255,
}
impl TryFrom<(u8, u8)> for DeviceSubClass {
    type Error = anyhow::Error;

    fn try_from((subclass, _protocol): (u8, u8)) -> Result<Self, Self::Error> {
        Ok(match subclass {
            0 => Self::Reserved0,
            1 => Self::DirectLineControlModel,
            2 => Self::AbstractControlModel,
            3 => Self::TelephoneControlModel,
            4 => Self::MultiChannelControlModel,
            5 => Self::CAPIControlModel,
            6 => Self::EthernetNetworkingControlModel,
            7 => Self::ATMNetworkingControlModel,
            8 => Self::WirelessHandsetControlModel,
            9 => Self::DeviceManagement,
            10 => Self::MobileDirectLineModel,
            11 => Self::OBEX,
            12 => Self::EthernetEmulationModel,
            13 => Self::NetworkControlModel,
            14..=127 => Self::ReservedForFutureUse(subclass),
            255 => Self::Unkown255,
            _ => Self::VendorSpecific(subclass),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceSubClass {
    pub subclass: u8,
    pub protocol: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataInterfaceSubClass {
    pub subclass: u8,
    pub protocol: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceDescriptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointDescriptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassSpecificDescriptor {
    Interface(InterfaceDescriptor),
    Endpoint(EndpointDescriptor),
}
impl ClassSpecificDescriptor {
    pub fn parse(response: &[u8]) -> anyhow::Result<(&[u8], Self)> {
        let (desc_length, desc_type) = response
            .iter()
            .cloned()
            .tuples()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Truncated descriptor"))?;

        println!("CDC descriptor: {:x?}", &response[..desc_length.into()]);
        Ok((
            &response[desc_length.into()..],
            match desc_type {
                0x24 => Self::Interface(InterfaceDescriptor),
                0x25 => Self::Endpoint(EndpointDescriptor),
                _ => anyhow::bail!(
                    "Invalid endpoint type ({}) for CDC specific interface class",
                    desc_type
                ),
            },
        ))
    }
}

pub struct CdCEndpoint(pub u8);

impl super::Endpoint for CdCEndpoint {
    fn update(
        &mut self,
        _timestamp: f64,
        transaction: Transaction,
    ) -> Option<anyhow::Result<super::DeviceEvent>> {
        let Transaction {
            token,
            data,
            handshake,
        } = transaction;
        data.and_then(|data| {
            if let HandShake::Ack = handshake {
                let ev: super::ClassEvent =
                    if let crate::usb::types::TokenType::In = token.token_type {
                        Event::Rx(data.payload)
                    } else {
                        Event::Tx(data.payload)
                    }
                    .into();
                Some(Ok(super::InterfaceEvent::Class(ev).into()))
                //Event::Rx()
                //println!(
                //"{:.9}: {} {:?} ({}) {:?}",
                //timestamp,
                //self.0,
                //transaction.token.token_type,
                //data.payload.len(),
                //std::str::from_utf8(&data.payload)
                //);
            } else {
                None
            }
        })
    }
}
