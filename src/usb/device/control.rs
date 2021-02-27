#![allow(dead_code)]

use super::types::*;
use crate::usb::protocol::Transaction;
use crate::usb::types::*;

use anyhow::anyhow;
use std::collections::HashMap;
use std::convert::TryFrom;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {}
impl From<Event> for super::DeviceEvent {
    fn from(from: Event) -> super::DeviceEvent {
        super::DeviceEvent::Control(from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Unknown(Vec<u8>),
    Descriptor(Descriptor),
}

#[derive(Debug, Clone)]
enum RequestState {
    Idle,
    Data(Request, Option<Vec<u8>>),
    // (_,_, is_early_status)
    Status(Request, Option<Vec<u8>>, bool),
}

pub struct ControlEndpoint {
    // request state
    request_state: RequestState,
}

impl ControlEndpoint {
    pub fn new() -> Self {
        Self {
            request_state: RequestState::Idle,
        }
    }
}

/// Implements the control channel 0
impl ControlEndpoint {
    pub(super) fn update(
        &mut self,
        _timestamp: f64,
        transaction: Transaction,
        endpoints: &mut HashMap<usize, Box<dyn super::Endpoint>>,
    ) -> Option<anyhow::Result<super::DeviceEvent>> {
        // dirty
        if endpoints.is_empty() {
            endpoints.insert(
                1,
                Box::new(super::cdc::CdCEndpoint(4)) as Box<dyn super::Endpoint>,
            );
            endpoints.insert(
                2,
                Box::new(super::cdc::CdCEndpoint(5)) as Box<dyn super::Endpoint>,
            );
            endpoints.insert(
                3,
                Box::new(super::cdc::CdCEndpoint(5)) as Box<dyn super::Endpoint>,
            );
            endpoints.insert(
                6,
                Box::new(super::cdc::CdCEndpoint(5)) as Box<dyn super::Endpoint>,
            );
        }
        macro_rules! bail {
            ($self:expr, $($tok:tt)*) => {{
                return {$self.request_state = RequestState::Idle;
                 Some(Err($($tok)*))}
            }}
        }

        match transaction.handshake {
            HandShake::NAck => return None,
            HandShake::Stall => {
                bail!(
                    self,
                    anyhow!(
                        "Stalled by {:?} while in {:x?}",
                        transaction,
                        self.request_state
                    )
                );
            }
            HandShake::Ack => {}
            _ => bail!(self, anyhow!("Woops ! {:?}", transaction)),
        }

        //println!("{:?}", self.request_state);

        loop {
            match (&mut self.request_state, transaction.token.token_type) {
                (RequestState::Idle, TokenType::Setup) => {
                    let payload: &[u8] = match transaction.data {
                        Some(ref data) => &data.payload,
                        None => bail!(self, anyhow!("missing device request")),
                    };

                    let request = match Request::try_from(payload) {
                        Ok(request) => request,
                        Err(e) => bail!(self, e),
                    };

                    if request.length != 0 {
                        self.request_state = RequestState::Data(request, None);
                    } else {
                        self.request_state = RequestState::Status(request, None, false);
                    }
                    break;
                }
                (_, TokenType::Setup) => {
                    let state = self.request_state.clone();
                    bail!(
                        self,
                        anyhow!("Unexpected Setup transaction while in {:x?}", state)
                    )
                }
                (RequestState::Data(ref mut request, ref mut buffer), TokenType::In)
                | (RequestState::Data(ref mut request, ref mut buffer), TokenType::Out) => {
                    match (request.direction, transaction.token.token_type) {
                        (DataPhaseTransferDirection::In, TokenType::In) => {}
                        (DataPhaseTransferDirection::Out, TokenType::Out) => {}
                        (DataPhaseTransferDirection::In, TokenType::Out)
                        | (DataPhaseTransferDirection::Out, TokenType::In) => {
                            let payload = buffer.take();
                            let request = *request;
                            self.request_state = RequestState::Status(request, payload, true);
                            continue;
                        }
                        (_, tt) => {
                            let err =
                                anyhow!("Unexpected {:x?} while in {:x?}", tt, self.request_state);
                            bail!(self, err);
                        }
                    }

                    if let Some(data) = transaction.data {
                        let buf =
                            buffer.get_or_insert_with(|| Vec::with_capacity(request.length.into()));

                        if data.payload.len() + buf.len() > buf.capacity() {
                            bail!(self, anyhow!("combined payload exceed expected size."));
                        }
                        let is_zlp = data.payload.is_empty();
                        buf.extend(data.payload);

                        if is_zlp {
                            println!("got a zlp",);
                        }
                        if is_zlp || buf.len() == buf.capacity() {
                            self.request_state =
                                RequestState::Status(*request, buffer.take(), false);
                        }

                        break;
                    } else {
                        let remaining = buffer
                            .as_ref()
                            .map(|buffer| buffer.capacity() - buffer.len())
                            .unwrap_or(0);
                        bail!(
                            self,
                            anyhow!(
                                "Empty data transaction while expecting {} more byte(s).",
                                remaining
                            )
                        );
                    }
                }
                (
                    RequestState::Status(ref mut request, ref mut buffer, is_early_status),
                    TokenType::In,
                )
                | (
                    RequestState::Status(ref mut request, ref mut buffer, is_early_status),
                    TokenType::Out,
                ) => {
                    if request.direction == DataPhaseTransferDirection::Out && *is_early_status {
                        let err =
                            anyhow!("Unexpected early status in {:x?}: {:x?}", request, buffer);
                        bail!(self, err);
                    }

                    let data = match &transaction.data {
                        Some(data) => data,
                        None => {
                            let err = anyhow!(
                                "Missing data phase in expected status transaction {:x?}: {:x?}",
                                request,
                                transaction
                            );
                            bail!(self, err)
                        }
                    };
                    if !data.payload.is_empty() {
                        let err = anyhow!(
                            "Unexpected payload is status' data phase: {:x?}: {:x?}",
                            request,
                            transaction
                        );
                        bail!(self, err)
                    }
                    if data.pid != DataPID::Data1 {
                        let err = anyhow!(
                            "Invalid PID for data phase of a status transaction: {:x?}: {:x?}",
                            request,
                            transaction
                        );
                        bail!(self, err)
                    }

                    let _request = *request;
                    let _buffer = buffer.take();
                    self.request_state = RequestState::Idle;

                    let response = _buffer.map(|buffer| -> Box<dyn std::fmt::Debug> {
                        match _request.request {
                            ERequest::Device(DeviceRequest::Standard(
                                StandardRequest::GetDescriptor(descriptor_type),
                            )) => match Descriptor::try_from((descriptor_type, buffer)) {
                                Ok(desc) => Box::new(desc),
                                Err(e) => Box::new(e),
                            },
                            _ => Box::new(buffer),
                        }
                    });

                    println!("{:.9}: {:x?}: {:x?}", _timestamp, _request, response);
                    break;
                    //if let Request { request_type: RequestType::Standard, request: RequestGet, value, index, length }

                    //return Some(Ok(Operation::Control(
                    //Request::Request(request),
                    //buffer.map(Response::Unknown),
                    //)));
                }

                (_, _) => {
                    // typically when a preview transaction failed at parsing and returned the
                    // controller to "Idle"
                    bail!(
                        self,
                        anyhow!("{:x?} while in {:x?}", transaction, self.request_state)
                    );
                }
            }
        }
        None
    }
}
