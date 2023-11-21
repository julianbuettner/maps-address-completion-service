#![doc = include_str!("../README.md")]
use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use log::{error, info};
use parse::stdin_stdout_database;
use serve::serve;

use crate::{compress::read_and_compress, parse_coordinates::process_osm_pdf_to_stdout};

mod api;
mod compress;
mod parse;
mod serve;
mod sorted_vec;
mod autofix;
mod parse_coordinates;
mod verbose_reader;


pub const MAX_ITEMS_HEADER: &str = "max-items";

#[derive(Parser, Debug)]
struct ParseParameters {
    /// File n .osm.pbf format
    #[arg(short, long)]
    input: PathBuf,
}

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
    /// Parse a *.osm.pbf file, json lines will be written to stdout
    Parse(ParseParameters),
    /// Read json lines, write compressed world object to stdout
    Compress(CompressParamters),
    /// Serve a world object via HTTP
    Serve(ServeParameters),
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
        .filter(|e| e.target() == "macs" || e.target().starts_with("macs::"))
        .level(log::LevelFilter::Debug)
        .chain(std::io::stderr())
        .apply()
        .unwrap()
}

fn main() -> Result<(), ()> {
    setup_logger();
    let args = Args::parse();

    match args.build {
        Subcommand::Parse(parse) => {
            info!("Reading osm.pbf from stdin...");
            let x = process_osm_pdf_to_stdout(parse.input);
            match x {
                Err(e) => error!("Error: {}", e),
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
