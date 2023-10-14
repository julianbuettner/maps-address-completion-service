use std::{mem::size_of, thread::sleep, time::Duration};

use parse::stdin_stdout_database;
use clap::Parser;

use crate::compress::read_and_compress;

mod parse;
mod compress;
mod sorted_vec;

#[derive(Parser, Debug)]
struct BuildParameters {}

#[derive(Parser, Debug)]
struct ServeParameters {}

#[derive(Parser, Debug)]
struct CompressParamters {}

#[derive(Parser, Debug)]
enum Subcommand {
    Parse(BuildParameters),
    Serve(ServeParameters),
    Compress(CompressParamters),
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    pub build: Subcommand,
}

enum CompactAdd {
    Data((u8, [u8; 14])),
    Heap(Box<Vec<u8>>),
}

fn main() {
    let args = Args::parse();

    match args.build {
        Subcommand::Parse(_) => {
            eprintln!("Reading osm.pbf from stdin...");
            let x = stdin_stdout_database();
            match x {
                Err(e) => eprintln!("Error: {}", e),
                Ok(()) => eprintln!("Done!"),
            }
        }
        Subcommand::Serve(_) => {
            todo!()
        }
        Subcommand::Compress(_) => {
            if let Err(e) = read_and_compress().unwrap();
        }
    }
}
