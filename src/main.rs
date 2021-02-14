use itertools::Itertools;

//mod serial;
//mod spi;
//mod spif;
mod usb;
//mod wizfi310;

mod pipeline;
mod sink;
mod source;

const TOP_LEVEL_SUBCOMMANDS: [&'static str; 12] = [
    "vcd",
    "logic",
    "logic2",
    "spi",
    "spif",
    "serial",
    "wizfi310",
    "usb::signal",
    "usb::byte",
    "usb::packet",
    "usb::protocol",
    "usb::device",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipeline = Vec::new();

    for (sub_command, args) in std::env::args().skip(1).peekable().batching(|it| {
        it.next().map(|subcmd| {
            let mut args = it
                .peeking_take_while(|s| !TOP_LEVEL_SUBCOMMANDS.contains(&s.as_str()))
                .collect::<Vec<_>>();

            if it.len() == 0 {
                args.push("-v".into());
            }
            (subcmd, args)
        })
    }) {
        match sub_command.as_str() {
            "vcd" => source::vcd::build(&mut pipeline, &args),
            "logic" => source::logic::build(&mut pipeline, &args),
            "logic2" => source::logic2::build(&mut pipeline, &args),
            "usb::signal" => usb::signal::build(&mut pipeline, &args),
            "usb::byte" => usb::byte::build(&mut pipeline, &args),
            "usb::packet" => usb::packet::build(&mut pipeline, &args),
            "usb::protocol" => usb::protocol::build(&mut pipeline, &args),
            "usb::device" => usb::device::build(&mut pipeline, &args),
            _ => unimplemented!(),
        }
    }

    assert_eq!(
        pipeline.len(),
        1,
        "The pipeline should resolve to a single iterator"
    );
    colored::control::set_override(true);
    if let Some(event_iterator) = pipeline.pop() {
        event_iterator.for_each(|_| {});
    }

    Ok(())
}
