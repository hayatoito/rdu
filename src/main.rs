use clap::{App, Arg};

// https://github.com/uutils/coreutils/blob/master/src/du/du.rs

fn app() -> App<'static, 'static> {
    App::new("rdu")
        .version("1.0")
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::with_name("apparent-size")
                .long("apparent-size")
                .help("Print apparent sizes, rather than disk usage"),
        )
        .arg(
            Arg::with_name("block-size")
                .short("B")
                .long("block-size")
                .takes_value(true)
                .help("scale sizes  by  SIZE before printing them"),
        )
        .arg(
            Arg::with_name("total")
                .short("c")
                .long("total")
                .help("Produce a grand total"),
        )
        .arg(
            Arg::with_name("human-readable")
                .short("h")
                .long("human-readable")
                .help("Print sizes in human readable format (e.g., 1K 234M 2G)"),
        )
        .arg(
            Arg::with_name("si")
                .long("si")
                .help("Like -h, but use powers of 1000 not 1024"),
        )
        .arg(
            Arg::with_name("max-depth")
                .short("d")
                .long("max-depth")
                .takes_value(true)
                .help(
                    "Print the total for a directory (or file, with --all)
            only if it is N or fewer levels below the command
            line argument",
                ),
        )
        .arg(Arg::with_name("file").multiple(true))
}

fn main() {
    let matches = app().get_matches();
    loggerv::init_with_verbosity(matches.occurrences_of("v")).unwrap();
    let config = rdu::Config {
        files: matches.values_of("file").unwrap().collect(),
        apparent_size: matches.is_present("apparent-size"),
        total: matches.is_present("total"),
        human_readable: matches.is_present("human-readable"),
        block_size: matches
            .value_of("block-size")
            .map(|d| d.parse().expect("block-size should be int")),
        si: matches.is_present("si"),
        max_depth: matches
            .value_of("max-depth")
            .map(|d| d.parse().expect("max-depth should be int")),
    };
    println!("config: {:?}", config);
    rdu::run(config);
}
