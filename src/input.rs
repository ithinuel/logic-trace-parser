mod logic2;
mod logicdata;
mod vcd;

use clap::ArgMatches;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample(pub u64);

pub fn sample_iterator(
    path: &str,
    matches: &ArgMatches<'_>,
) -> anyhow::Result<impl Iterator<Item = (f64, anyhow::Result<Sample>)>> {
    let it: Box<dyn Iterator<Item = (f64, anyhow::Result<Sample>)>> = if matches.is_present("vcd") {
        Box::new(vcd::VcdParser::new(std::fs::File::open(path)?))
    } else if matches.is_present("logic2") {
        Box::new(logic2::LogicData::new(path, matches)?)
    } else {
        Box::new(logicdata::LogicDataParser::new(
            std::fs::File::open(path)?,
            matches,
        ))
    };
    Ok(it)
}
