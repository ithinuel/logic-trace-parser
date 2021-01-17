#![allow(dead_code)]
use std::fmt::Debug;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TokenType {
    Setup,
    Out,
    In,
    Ping,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub address: u8,
    pub endpoint: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataPID {
    Data0,
    Data1,
    Data2,
    MData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub pid: DataPID,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandShake {
    Ack,
    NAck,
    Stall,
    NYet,
    Err,
}

pub fn crc5(v: &[u8]) -> u8 {
    let mut acc = 0x1F;
    for b in v {
        let mut b = *b;
        for _ in 0..8 {
            let do_xor = (b & 1) != ((acc >> 4) & 1);
            acc <<= 1;
            if do_xor {
                acc ^= 5
            }
            acc &= 0x1F;
            b >>= 1;
        }
    }
    acc
}

pub fn crc16(v: &[u8]) -> u16 {
    let mut acc = 0xFFFF;
    for b in v {
        let mut b = *b;
        for _ in 0..8 {
            let do_xor = (b as u16 & 1) != ((acc >> 15) & 1);
            acc <<= 1;
            if do_xor {
                acc ^= 0x8005
            }
            b >>= 1;
        }
    }
    acc
}
