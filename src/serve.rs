use log::{error, info};
use std::{
    fs::OpenOptions,
    io::BufReader,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    process::exit,
};
use tokio::runtime::Builder;

use crate::{api::get_app, compress::World};

fn parse_into_world(f: PathBuf) -> Result<World, String> {
    info!("Loading from world file {:?}...", f);
    let reader = OpenOptions::new()
        .read(true)
        .create_new(false)
        .open(f)
        .map_err(|e| e.to_string())?;
    let buf_reader = BufReader::new(reader);
    bincode::deserialize_from(buf_reader).map_err(|e| e.to_string())
}

async fn start_server(w: World, ip: IpAddr, port: u16) -> ! {
    let app = get_app(w);
    let addr = SocketAddr::from((ip, port));
    info!("Serve on {}:{}...", ip, port);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    unreachable!("Server did terminate against expectations.");
}

pub fn serve(world_file: PathBuf, ip: IpAddr, port: u16) -> ! {
    if !world_file.exists() {
        error!("File {:?} not found.", world_file);
        exit(1);
    }
    let world = parse_into_world(world_file);
    if let Err(e) = world {
        error!("Error parsing world file: {}", e);
        exit(1);
    }
    let world = world.unwrap();
    info!("World loadded, containing {} countries.", world.count());
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();
    rt.block_on(start_server(world, ip, port))
}
