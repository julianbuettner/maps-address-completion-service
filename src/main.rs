use std::{thread::sleep, time::Duration};

use build_database::stdin_stdout_database;
use clap::Parser;
use serve::build;

mod build_database;
mod serve;

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

fn main() {
    let args = Args::parse();

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
            let mut _res = build().unwrap();
            println!("Shrink in 10s");
            sleep(Duration::from_secs(20));
            _res.shrink();
            sleep(Duration::from_secs(900));
        },
    }
}
