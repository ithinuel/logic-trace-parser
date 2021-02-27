use std::convert::TryInto;
use std::io::Read;

use anyhow::{anyhow, Result};
use clap::Arg;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;

use super::Sample;
use crate::pipeline::{Event, EventIterator};

#[derive(Debug)]
struct Channel {
    id: u32,
    initial_state: bool,
    transitions: Vec<f64>,
}

pub struct LogicData<T> {
    transitions: T,
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

pub fn new_parser(path: &str) -> Result<LogicData<impl Iterator<Item = (f64, u64)>>> {
    // display something while processing
    let progress_bar = ProgressBar::new(!0);
    progress_bar.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ")
            .template(" {spinner} {msg}"),
    );
    progress_bar.set_message("Processing transitions");
    progress_bar.enable_steady_tick(80);

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

                // transitions is known to be a multiple of 8. array_chunks will make this cleaner
                transitions: transitions
                    .chunks(8)
                    .map(move |buf| buf.try_into().map(f64::from_le_bytes).unwrap())
                    .collect(),
            }))
        })
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, _>>()?;

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
        .filter_map(|chan| chan.transitions.first().copied())
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .ok_or_else(|| anyhow::anyhow!("No sample found !"))?;

    // lazily process
    let transitions = channels
        .into_iter()
        .map(|channel| {
            let Channel {
                id, transitions, ..
            } = channel;
            transitions.into_iter().map(move |ts| (id, ts))
        })
        .kmerge_by(|(_, a_ts), (_, b_ts)| a_ts < b_ts)
        .peekable()
        .batching(move |it| {
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

            new_ts.map(|ts| {
                current_ts = ts;
                current_state ^= mask;
                (ts, current_state)
            })
        });

    Ok(LogicData { transitions })
}

impl<T: Iterator<Item = (f64, u64)>> Iterator for LogicData<T> {
    type Item = Event;
    fn next(&mut self) -> Option<Self::Item> {
        let (ts, sample) = self.transitions.next()?;
        Some((ts, Ok(Box::new(Sample(sample)))))
    }
}

impl<T: Iterator<Item = (f64, u64)> + 'static> EventIterator for LogicData<T> {
    fn into_iterator(self: Box<Self>) -> Box<dyn Iterator<Item = Event>> {
        self
    }
    fn event_type(&self) -> std::any::TypeId {
        std::any::TypeId::of::<super::Sample>()
    }
    fn event_type_name(&self) -> &'static str {
        std::any::type_name::<super::Sample>()
    }
}

pub fn build(pipeline: &mut Vec<Box<dyn EventIterator>>, args: &[String]) {
    let args = clap::SubCommand::with_name("logic2")
        .setting(clap::AppSettings::NoBinaryName)
        .arg(
            Arg::with_name("file")
                .help("Input file. (may be a folder in case of Saleae Logic 2 exports.)")
                .required(true),
        )
        .get_matches_from(args);

    let parser = Box::new(new_parser(args.value_of("file").unwrap()).unwrap());
    pipeline.push(parser);
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
