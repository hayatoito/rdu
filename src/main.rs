extern crate rdu;
extern crate loggerv;
extern crate clap;

use clap::{App, Arg};

// https://github.com/uutils/coreutils/blob/master/src/du/du.rs

fn app() -> App<'static, 'static> {
    App::new("rdu")
        .version("1.0")
        .arg(Arg::with_name("v")
            .short("v")
            .multiple(true)
            .help("Sets the level of verbosity"))
        .arg(Arg::with_name("DIR")
             .multiple(true))
}

fn main() {
    let matches = app().get_matches();
    loggerv::init_with_verbosity(matches.occurrences_of("v")).unwrap();
    // let files = matches.value_of("file").unwrap();
    // let dir = matches.value_of("file").unwrap();
    rdu::run(".");
}
