use crate::source::Sample;
use clap::{value_t, App, Arg, ArgMatches, SubCommand};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Polarity {
    High,
    Low,
}
impl FromStr for Polarity {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "High" => Ok(Polarity::High),
            "Low" => Ok(Polarity::Low),
            _ => Err("no match"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Phase {
    FirstEdge,
    SecondEdge,
}

#[derive(Debug)]
pub enum SpiEvent {
    ChipSelect(bool),
    Data { mosi: u8, miso: u8 },
}

#[derive(Debug)]
pub struct SpiBuilder {
    cs: u8,
    mosi: u8,
    miso: u8,
    clk: u8,
    phase: Phase,
    polarity: Polarity,
    cs_active_level: Polarity,
}
impl SpiBuilder {
    pub fn new() -> Self {
        Self {
            cs: 0,
            mosi: 1,
            miso: 2,
            clk: 3,
            phase: Phase::FirstEdge,
            polarity: Polarity::High,
            cs_active_level: Polarity::Low,
        }
    }
    pub fn cs(mut self, cs: u8) -> Self {
        self.cs = cs;
        self
    }
    pub fn clk(mut self, clk: u8) -> Self {
        self.clk = clk;
        self
    }
    pub fn miso(mut self, miso: u8) -> Self {
        self.miso = miso;
        self
    }
    pub fn mosi(mut self, mosi: u8) -> Self {
        self.mosi = mosi;
        self
    }
    pub fn mode(mut self, phase: Phase, polarity: Polarity) -> Self {
        self.phase = phase;
        self.polarity = polarity;
        self
    }
    pub fn cs_active_level(mut self, cs_active_level: Polarity) -> Self {
        self.cs_active_level = cs_active_level;
        self
    }
    pub fn into_spi<T>(self, it: T) -> Spi<T> {
        Spi {
            it,
            pending_event: None,

            ccs: self.cs,
            cmiso: self.miso,
            cmosi: self.mosi,
            cclk: self.clk,

            clk_phase: self.phase == Phase::SecondEdge,
            clk_polarity: self.polarity == Polarity::Low,
            cs_active_level: self.cs_active_level == Polarity::High,

            shift_cnt: 0,
            shift_reg_mosi: 0,
            shift_reg_miso: 0,
            clk: false,
            cs: false,
        }
    }
}

pub struct Spi<T> {
    it: T,
    pending_event: Option<(f64, SpiEvent)>,

    ccs: u8,
    cmiso: u8,
    cmosi: u8,
    cclk: u8,

    cs_active_level: bool,
    clk_phase: bool,
    clk_polarity: bool,

    shift_reg_mosi: u8,
    shift_reg_miso: u8,
    shift_cnt: u8,
    clk: bool,
    cs: bool,
}
impl<T> fmt::Debug for Spi<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Spi {{ mosi: {:02x}, miso: {:02x}, cnt: {} }}",
            self.shift_reg_mosi, self.shift_reg_miso, self.shift_cnt
        )
    }
}
impl<T> Iterator for Spi<T>
where
    T: Iterator<Item = (f64, anyhow::Result<Sample>)>,
{
    type Item = (f64, anyhow::Result<SpiEvent>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut ret = self.pending_event.take();

        while ret.is_none() {
            let (ts, sample) = match self.it.next()? {
                (ts, Ok(Sample(sample))) => (ts, sample),
                (ts, Err(e)) => return Some((ts, Err(e))),
            };
            let clk = ((sample >> self.cclk) & 1) == 1;
            let cs = ((sample >> self.ccs) & 1) == 1;

            if cs != self.cs {
                self.cs = cs;

                ret = Some((ts, SpiEvent::ChipSelect(cs)));
                if cs {
                    self.shift_cnt = 0;
                }
            }
            if clk != self.clk {
                self.clk = clk;
                if cs == self.cs_active_level && clk != (self.clk_phase ^ self.clk_polarity) {
                    self.shift_reg_mosi =
                        self.shift_reg_mosi.wrapping_shl(1) | (((sample >> self.cmosi) & 1) as u8);
                    self.shift_reg_miso =
                        self.shift_reg_miso.wrapping_shl(1) | (((sample >> self.cmiso) & 1) as u8);
                    self.shift_cnt += 1;

                    if self.shift_cnt == 8 {
                        self.shift_cnt = 0;

                        let event = Some((
                            ts,
                            SpiEvent::Data {
                                mosi: self.shift_reg_mosi,
                                miso: self.shift_reg_miso,
                            },
                        ));
                        if ret.is_some() {
                            self.pending_event = event;
                        } else {
                            ret = event;
                        }
                    }
                }
            }
        }
        ret.map(|(ts, event)| (ts, Ok(event)))
    }
}

impl<T> Spi<T> {
    pub fn new<'a>(input: T, matches: &ArgMatches<'a>) -> Spi<T> {
        let (phase, polarity) = match value_t!(matches, "mode", u8).unwrap_or_else(|e| e.exit()) {
            1 => (Phase::SecondEdge, Polarity::High),
            2 => (Phase::FirstEdge, Polarity::Low),
            3 => (Phase::SecondEdge, Polarity::Low),
            0 | _ => (Phase::FirstEdge, Polarity::High),
        };

        SpiBuilder::new()
            .cs(value_t!(matches, "cs", u8).unwrap_or_else(|e| e.exit()))
            .miso(value_t!(matches, "miso", u8).unwrap_or_else(|e| e.exit()))
            .mosi(value_t!(matches, "mosi", u8).unwrap_or_else(|e| e.exit()))
            .clk(value_t!(matches, "clk", u8).unwrap_or_else(|e| e.exit()))
            .mode(phase, polarity)
            .cs_active_level(
                value_t!(matches, "cs_active_level", Polarity).unwrap_or_else(|e| e.exit()),
            )
            .into_spi(input)
    }
}

pub trait SpiIteratorExt: Sized {
    fn into_spi(self, matches: &ArgMatches) -> Spi<Self> {
        Spi::new(self, matches)
    }
}
impl<T> SpiIteratorExt for T where T: Iterator<Item = (f64, anyhow::Result<Sample>)> {}

pub fn args() -> [Arg<'static, 'static>; 6] {
    [
        Arg::from_usage("--cs [cs] 'Channel used for the chip select.'").default_value("0"),
        Arg::from_usage("--miso [miso] 'Channel used for miso'").default_value("1"),
        Arg::from_usage("--mosi [mosi] 'Channel used for mosi'").default_value("2"),
        Arg::from_usage("--clk [clk] 'Channel used for the clock'").default_value("3"),
        Arg::from_usage("-l, --cs_active_level [cs_active_level] 'Chip select active level'")
            .possible_values(&["High", "Low"])
            .default_value("Low"),
        Arg::from_usage("-m --mode [mode] 'Spi mode'")
            .possible_values(&["0", "1", "2", "3"])
            .default_value("0"),
    ]
}

pub fn subcommand() -> App<'static, 'static> {
    SubCommand::with_name("spi").args(&args())
}
