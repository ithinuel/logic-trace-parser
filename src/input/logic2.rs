use anyhow::{anyhow, Result};
use clap::ArgMatches;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use std::convert::TryInto;
use std::io::Read;

use super::Sample;

#[derive(Debug)]
struct Channel {
    id: u32,
    initial_state: bool,

    transitions: Vec<u8>,
}

pub struct LogicData {
    transitions: Vec<(f64, u64)>,
    rptr: usize,
}

fn parse_common_header(buf: &[u8]) -> anyhow::Result<(u32, u32)> {
    anyhow::ensure!(buf.len() == 16, "Incomplete file header");
    anyhow::ensure!(buf.starts_with(b"<SALEAE>"), "Invalid prefix");

    let version = buf[8..12].try_into().map(u32::from_le_bytes).unwrap();
    let file_type = buf[12..].try_into().map(u32::from_le_bytes).unwrap();
    Ok((version, file_type))
}

fn parse_digital_header(buf: &[u8]) -> anyhow::Result<(u32, f64, f64, u64)> {
    anyhow::ensure!(buf.len() == 28, "Incomplete file header");

    let initial_state = buf[..4].try_into().map(u32::from_le_bytes).unwrap();
    let begin_time = buf[4..12].try_into().map(f64::from_le_bytes).unwrap();
    let end_time = buf[12..20].try_into().map(f64::from_le_bytes).unwrap();
    let num_transitions = buf[20..].try_into().map(u64::from_le_bytes).unwrap();
    Ok((initial_state, begin_time, end_time, num_transitions))
}

impl LogicData {
    pub fn new(path: &str, _matches: &ArgMatches<'_>) -> Result<Self> {
        // select valid files
        let channels = std::fs::read_dir(path)?
            .map(|entry| -> anyhow::Result<_> {
                let entry = entry?;

                // ignore non-files entries
                if !std::fs::metadata(entry.path())?.is_file() {
                    return Ok(None);
                }

                let file_name = if let Some(file_name) = entry.file_name().to_str() {
                    file_name.to_owned()
                } else {
                    return Ok(None);
                };

                let chan_id = file_name
                    .strip_prefix("digital_")
                    .and_then(|s| s.strip_suffix(".bin"))
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| anyhow!("Invalid filename format {:?}", file_name))?;

                let mut file = std::fs::File::open(entry.path())?;
                let mut buf = [0; 32];
                let initial_state = {
                    let len = file.read(&mut buf[..16])?;
                    match parse_common_header(&buf[..len])? {
                        (0, 0) => {}
                        (0, d) => return Err(anyhow!("Unexpected file type {}.", d)),
                        (v, _) => return Err(anyhow!("Unsupported file format version {}.", v)),
                    }

                    let len = file.read(&mut buf[..28])?;
                    parse_digital_header(&buf[..len])?.0
                };

                let mut transitions = Vec::new();
                if file.read_to_end(&mut transitions)? % 8 != 0 {
                    anyhow::bail!("Corrupted file");
                }

                Ok(Some(Channel {
                    id: chan_id,
                    initial_state: initial_state == 1,
                    transitions,
                }))
            })
            .filter_map(Result::transpose)
            .collect::<Result<Vec<_>, _>>()?;

        Self::parse_data(channels)
    }

    fn parse_data(channels: Vec<Channel>) -> Result<Self> {
        // display something while processing
        let progress_bar = ProgressBar::new(!0);
        progress_bar.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
                .template(" {spinner} {msg}"),
        );
        progress_bar.set_message("Processing transitions");
        progress_bar.enable_steady_tick(80);

        // compute initial_state
        let mut current_state = channels.iter().fold(0, |acc, c| {
            acc | {
                if c.initial_state {
                    1 << c.id
                } else {
                    0
                }
            }
        });
        let mut current_ts = channels
            .iter()
            .filter_map(|chan| {
                chan.transitions[..8]
                    .try_into()
                    .map(f64::from_le_bytes)
                    .ok()
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .ok_or_else(|| anyhow::anyhow!("No sample found !"))?;

        // process
        // Note: we could stream that if we had a way to make Chunks take ownership of the
        // Vec/slice.
        let mut transitions: Vec<_> = vec![(0., current_state)];
        transitions.extend(
            channels
                .iter()
                .map(|channel| {
                    let Channel {
                        id, transitions, ..
                    } = channel;
                    transitions
                        .chunks(8)
                        .map(move |buf| (*id, buf.try_into().map(f64::from_le_bytes).unwrap()))
                })
                .kmerge_by(|(_, a_ts), (_, b_ts)| a_ts < b_ts)
                .peekable()
                .batching(|it| {
                    let mut mask = 0;
                    let mut new_ts = None;
                    it.peeking_take_while(|(id, ts)| {
                        let bit = 1 << id;
                        let prev_ts = *new_ts.get_or_insert(*ts);

                        assert!(prev_ts <= *ts);
                        if (mask & bit) != bit && (ts - prev_ts) < 0.000_000_001 {
                            mask |= bit;
                            true
                        } else {
                            false
                        }
                    })
                    .for_each(|_| {});

                    new_ts.take().map(|ts| {
                        current_ts = ts;
                        current_state ^= mask;
                        (ts, current_state)
                    })
                }),
        );

        Ok(Self {
            transitions,
            rptr: 0,
        })
    }
}

impl Iterator for LogicData {
    type Item = (f64, anyhow::Result<Sample>);
    fn next(&mut self) -> Option<Self::Item> {
        let (ts, sample) = *self.transitions.get(self.rptr)?;
        self.rptr += 1;

        Some((ts, Ok(Sample(sample))))
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_incomplete_tag() {
        assert!(super::parse_common_header(b"<SAL").is_err());
    }
    #[test]
    fn can_parse_header() {
        assert_eq!(
            Some((1u32, 2u32)),
            super::parse_common_header(b"<SALEAE>\x01\x00\x00\x00\x02\x00\x00\x00").ok()
        );
    }

    #[test]
    fn can_parse_digital_header() {
        #[rustfmt::skip]
        let raw = &[
            1, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 55, 64,
            0, 0, 0, 0, 0, 128, 70, 64,
            67, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(
            Some((1u32, 23f64, 45f64, 67u64)),
            super::parse_digital_header(raw).ok()
        )
    }
}
