use clap::{App, Arg, ArgMatches, SubCommand};

use std::convert::TryFrom;

use super::byte::Byte;
use super::types::{crc16, crc5, Data, DataPID, HandShake, Token, TokenType};

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Reset,
    SoF(u16),
    HandShake(HandShake),
    Token(Token),
    Data(Data),
}

impl TryFrom<&[u8]> for Packet {
    type Error = anyhow::Error;
    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        anyhow::ensure!(buf[0] == 0x80, "Invalid sync byte");

        match &buf[1..] {
            &[0xA5, lsb, msb] => {
                anyhow::ensure!(crc5(&buf[2..]) == 0x0C, "Crc error");

                let frm_num = ((u16::from(msb) << 8) | u16::from(lsb)) & 0x7FF;
                Ok(Packet::SoF(frm_num))
            }
            &[pid @ 0xE1, lsb, msb]
            | &[pid @ 0x69, lsb, msb]
            | &[pid @ 0x2D, lsb, msb]
            | &[pid @ 0xB4, lsb, msb] => {
                anyhow::ensure!(crc5(&[lsb, msb]) == 0x0C, "Crc error");

                Ok(Packet::Token(Token {
                    token_type: if pid == 0xE1 {
                        TokenType::Out
                    } else if pid == 0x69 {
                        TokenType::In
                    } else if pid == 0x2D {
                        TokenType::Setup
                    } else {
                        TokenType::Ping
                    },
                    address: lsb & 0x7F,
                    endpoint: ((msb & 0x7) << 1) | (lsb >> 7),
                }))
            }
            &[0x78, _, _, _] => {
                anyhow::ensure!(crc5(&buf[2..]) == 0x0C, "Crc Error");

                unimplemented!("Split tokens are not supported");
            }

            // the extra 2 underscores are crc16 place holder
            &[pid @ 0xC3, ref data @ .., _, _]
            | &[pid @ 0x4B, ref data @ .., _, _]
            | &[pid @ 0x17, ref data @ .., _, _]
            | &[pid @ 0x0F, ref data @ .., _, _] => {
                anyhow::ensure!(crc16(&buf[2..]) == 0x800D, "CRC Error");
                Ok(Packet::Data(Data {
                    pid: if pid == 0xC3 {
                        DataPID::Data0
                    } else if pid == 0x4B {
                        DataPID::Data1
                    } else if pid == 0x17 {
                        DataPID::Data2
                    } else {
                        DataPID::MData
                    },
                    payload: data.to_vec(),
                }))
            }
            &[0xD2] => Ok(Packet::HandShake(HandShake::Ack)),
            &[0x5A] => Ok(Packet::HandShake(HandShake::NAck)),
            &[0x1E] => Ok(Packet::HandShake(HandShake::Stall)),
            &[0x96] => Ok(Packet::HandShake(HandShake::NYet)),
            &[0x3C] => Ok(Packet::HandShake(HandShake::Err)),

            _ => anyhow::bail!("Unknown packet {:x?}", buf),
        }
    }
}

pub struct PacketIterator<T> {
    it: T,
}

impl<T> Iterator for PacketIterator<T>
where
    T: Iterator<Item = (f64, anyhow::Result<Byte>)>,
{
    type Item = (f64, anyhow::Result<Packet>);
    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = Vec::new();
        let out = loop {
            match self.it.next()? {
                (ts, Ok(byte)) => match byte {
                    Byte::Reset => break (ts, Ok(Packet::Reset)),
                    Byte::Idle => {}
                    Byte::Byte(b) => buf.push(b),
                    Byte::Eop => break (ts, Packet::try_from(&buf as &[u8])),
                },
                (ts, Err(e)) => break (ts, Err(e)),
            }
        };
        Some(out)
    }
}

impl<T> PacketIterator<T> {
    pub fn new<'a>(input: T, _matches: &ArgMatches<'a>) -> Self {
        Self { it: input }
    }
}
pub trait PacketIteratorExt: Sized {
    fn into_packet(self, matches: &ArgMatches) -> PacketIterator<Self> {
        PacketIterator::new(self, matches)
    }
}
impl<T> PacketIteratorExt for T where T: Iterator<Item = (f64, anyhow::Result<Byte>)> {}

pub fn args() -> [Arg<'static, 'static>; 3] {
    crate::usb::byte::args()
}
pub fn subcommand() -> App<'static, 'static> {
    SubCommand::with_name("usb::packet").args(&args())
}
