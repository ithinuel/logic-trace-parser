# Logic trace parser

This tool parses an export from Saleae Logic software and tries to interpret it as a the selected communication.
```
logic-trace-parser 0.1.1
Wilfried Chauveau <wilfried.chauveau@arm.com>


USAGE:
    logic-trace-parser [OPTIONS] [file] [SUBCOMMAND]

OPTIONS:
    -f, --freq <freq>    Sample frequency (only used on binary input) [default: 1.]
    -h, --help           Prints help information
    -v                   Sets the level of verbosity
        --vcd            Input is a vcd file
    -V, --version        Prints version information

ARGS:
    <file>    Input file. If not provided, stdin will be used.

SUBCOMMANDS:
    help        Prints this message or the help of the given subcommand(s)
    serial      
    spi         
    spif        
    wizfi310
```
## How to use :

`ltp trace_sample_on_change.bin | rg -v '(StatusRegister|WriteEnable)' | less`

## TODO:

Things I'd like to implement at some point in the future:

- [ ] Source-sink pipelining
- [ ] usb::device::msd
- [ ] usb::device::cdc
- [ ] automatic usb speed detection
- [ ] SCSI
- [ ] ppp
