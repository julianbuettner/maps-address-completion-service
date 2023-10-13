use std::{thread::sleep, time::Duration, mem::size_of};

use build_database::stdin_stdout_database;
use clap::Parser;
use serve::build;

mod build_database;
mod serve;
mod sorted_vec;

#[derive(Parser, Debug)]
struct BuildParameters {}

#[derive(Parser, Debug)]
struct ServeParameters {}

#[derive(Parser, Debug)]
enum Subcommand {
    Build(BuildParameters),
    Serve(ServeParameters),
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

    println!("Sizeof usize: {}", size_of::<usize>());
    println!("Sizeof vec: {}", size_of::<Vec<String>>());
    println!("Sizeof Box: {}", size_of::<Box<Vec<String>>>());
    println!("Sizeof CompactAdd: {}", size_of::<CompactAdd>());
    return;

    match args.build {
        Subcommand::Build(_) => {
            eprintln!("Reading osm.pbf from stdin...");
            let x = stdin_stdout_database();
            match x {
                Err(e) => eprintln!("Error: {}", e),
                Ok(()) => eprintln!("Done!"),
            }
        }
        Subcommand::Serve(_) => {
            let mut res = build().unwrap();
            let x = res.len();
            eprintln!("Street count: {}", x);
        },
    }
}
