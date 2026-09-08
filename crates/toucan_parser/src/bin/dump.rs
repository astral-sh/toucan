//! Parse a C file and dump the AST.

extern crate toucan_parser;

use std::process::exit;

use toucan_parser::driver::{Config, Flavor};
use toucan_parser::visit::Visit;

fn main() {
    let mut config = Config::default();
    let mut source = None;
    let mut quiet = false;

    for opt in std::env::args().skip(1) {
        if opt == "-use-gcc" {
            config = Config::with_gcc();
        } else if opt == "-use-clang" {
            config = Config::with_clang();
        } else if opt == "-use-std" {
            config.flavor = Flavor::StdC11;
        } else if opt == "-q" {
            quiet = true;
        } else if opt.starts_with("-") {
            config.cpp_options.push(opt);
        } else {
            if source.is_none() {
                source = Some(opt);
            } else {
                println!("multiple input files given");
                exit(1);
            }
        }
    }

    let source = match source {
        Some(s) => s,
        None => {
            println!("input file required");
            exit(1);
        }
    };

    match toucan_parser::driver::parse(&config, &source) {
        Ok(parse) => {
            if !quiet {
                let mut buf = String::new();
                {
                    let mut printer = toucan_parser::print::Printer::new(&mut buf);
                    printer.visit_translation_unit(&parse.unit);
                }
                println!("{}", buf);
            }
        }
        Err(err) => {
            println!("{}", err);
            exit(1);
        }
    }
}
