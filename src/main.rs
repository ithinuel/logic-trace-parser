use anyhow::Context;
use clap::{crate_authors, crate_description, crate_name, crate_version, App, AppSettings, Arg};

mod input;
mod serial;
mod spi;
mod spif;
mod wizfi310;

use input::sample_iterator;
use serial::SerialIteratorExt as _;
use spi::SpiIteratorExt as _;
use spif::SpifIteratorExt as _;
use wizfi310::Wizfi310IteratorExt as _;

fn inspect_with_depth<T: core::fmt::Debug>(
    matches: &clap::ArgMatches,
    source: &'static str,
    depth: u64,
) -> impl Fn(&(f64, T)) {
    let verbosity = matches.occurrences_of("v");
    move |(ts, v)| {
        if verbosity >= depth {
            println!("{:.9}:{}: {:x?}", ts, source, v);
        }
    }
}
fn print<T: core::fmt::Debug>((ts, v): (f64, T)) {
    println!("{:.9}: {:x?}", ts, v)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new(crate_name!())
        .setting(AppSettings::UnifiedHelpMessage)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(Arg::from_usage("--vcd 'Input is a vcd file'").global(true))
        .arg(Arg::from_usage("--logic2 'Input is a Saleae Logic 2 capture file'").global(true))
        .subcommand(spi::subcommand())
        .subcommand(spif::subcommand())
        .subcommand(serial::subcommand())
        .subcommand(wizfi310::subcommand())
        .args(&[
            Arg::from_usage("-f, --freq [freq] 'Sample frequency (only used on binary input)'")
                .default_value("1.")
                .global(true),
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity")
                .global(true),
            Arg::with_name("file")
                .help("Input file. (may be a folder in case of Saleae Logic 2 exports.)")
                .required(true),
        ])
        .get_matches();

    let path = matches
        .value_of("file")
        .with_context(|| "clap guaranties this is available.")?;

    match matches.subcommand() {
        ("spif", Some(matches)) => sample_iterator(path, matches)?
            .inspect(inspect_with_depth(matches, "sample", 2))
            .into_spi(matches)
            .inspect(inspect_with_depth(matches, "spi", 1))
            .into_spif(matches)
            .for_each(|_| todo!()),
        ("spi", Some(matches)) => sample_iterator(path, matches)?
            .inspect(inspect_with_depth(matches, "sample", 1))
            .into_spi(matches)
            .for_each(print),
        ("serial", Some(matches)) => sample_iterator(path, matches)?
            .inspect(inspect_with_depth(matches, "sample", 1))
            .into_serial(matches)
            .for_each(print),
        ("wizfi310", Some(matches)) => sample_iterator(path, matches)?
            .inspect(inspect_with_depth(matches, "sample", 2))
            .into_serial(matches)
            .inspect(inspect_with_depth(matches, "serial", 1))
            .into_wizfi310(matches)
            .for_each(print),
        _ => unreachable!(),
    };
    Ok(())
}
