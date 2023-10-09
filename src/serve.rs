use std::{io::{self, BufRead, BufReader, Read}, collections::HashSet};

use crate::build_database::Address;

pub fn iter_items(io: impl Read) -> impl Iterator<Item = Result<Address, String>> {
    let mut buf_reader = BufReader::new(io);
    buf_reader
        .lines()
        // .map(|l| l.map_err(|e| e.to_string()).map(|v| serde_json::from_str(v.as_str())))
        .map(|line| match line {
            Ok(text) => match serde_json::from_str(text.as_str()) {
                Ok(addr) => Ok(addr),
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e.to_string()),
        })
}

pub fn serve() {
    eprintln!("Reading jsonl from stdin...");
    let stdin = io::stdin().lock();
    let mut street_names = HashSet::new();
    for it in iter_items(stdin) {
        let it = it.unwrap();
        street_names.insert(it.street);
    }

    println!("Count: {}", street_names.len());

    let mut v: Vec<String> = street_names.iter().cloned().collect();
    v.sort();
    for (i, stree) in v.iter().enumerate() {
        println!("{} - {}", i, stree);
    }

    println!("Count: {}", street_names.len());
}
