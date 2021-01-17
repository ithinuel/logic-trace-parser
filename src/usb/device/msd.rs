#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceSubClass {
    pub subclass: u8,
    pub protocol: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassSpecificDescriptor;
impl ClassSpecificDescriptor {
    pub fn parse(response: &[u8]) -> anyhow::Result<(&[u8], Self)> {
        Ok((&response[..response[0].into()], Self))
    }
}

pub struct MsdEndpoint;
impl super::Endpoint for MsdEndpoint {
    fn update(
        &mut self,
        _timestamp: f64,
        _transaction: super::protocol::Transaction,
    ) -> Option<anyhow::Result<super::DeviceEvent>> {
        None
    }
}
