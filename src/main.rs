use build_database::stdin_stdout_database;

mod build_database;

fn main() {
    eprintln!("Reading osm.pbf from stdin...");
    let x =  stdin_stdout_database();
    match x {
        Err(e) => eprintln!("Error: {}", e),
        Ok(()) => eprintln!("Done!"),
    }
}
