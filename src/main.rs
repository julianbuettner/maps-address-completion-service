#![doc = include_str!("../README.md")]
use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use log::{error, info};
use parse::stdin_stdout_database;
use serve::serve;

use crate::compress::read_and_compress;

mod api;
mod compress;
mod parse;
mod serve;
mod sorted_vec;


pub const MAX_ITEMS: usize = usize::MAX;

#[derive(Parser, Debug)]
struct BuildParameters {}

#[derive(Parser, Debug)]
struct ServeParameters {
    #[arg(short, long)]
    world: PathBuf,
    #[arg(short, long, default_value = "3000")]
    port: u16,
    #[arg(short, long, default_value = "127.0.0.1")]
    ip: IpAddr,
}

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

fn setup_logger() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stderr())
        .apply()
        .unwrap()
}

fn main() -> Result<(), ()> {
    setup_logger();
    let args = Args::parse();

    match args.build {
        Subcommand::Parse(_) => {
            info!("Reading osm.pbf from stdin...");
            let x = stdin_stdout_database();
            match x {
                Err(e) => error!("{}", e),
                Ok(()) => info!("Done!"),
            }
        }
        Subcommand::Serve(parameters) => serve(parameters.world, parameters.ip, parameters.port),
        Subcommand::Compress(_) => {
            if let Err(e) = read_and_compress() {
                error!("{}", e)
            }
        }
    }
    Ok(())
}
