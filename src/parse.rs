use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use log::info;
use osmpbfreader::{OsmObj, Tags};
use serde::{Deserialize, Serialize};

use crate::autofix::is_unfixable;

#[derive(Debug, Serialize, Deserialize)]
pub struct Address {
    pub country: String,
    pub city: String,
    pub postcode: String,
    pub street: String,
    pub housenumber: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IncompleteAddress {
    pub country: Option<String>,
    pub city: Option<String>,
    pub postcode: Option<String>,
    pub street: Option<String>,
    pub housenumber: Option<String>,
}

impl IncompleteAddress {
    pub fn is_complete(&self) -> bool {
        self.country.is_some()
            && self.city.is_some()
            && self.postcode.is_some()
            && self.street.is_some()
            && self.housenumber.is_some()
    }
    pub fn into_complete(self) -> Option<Address> {
        Some(Address {
            country: self.country?,
            city: self.city?,
            postcode: self.postcode?,
            street: self.street?,
            housenumber: self.housenumber?,
        })
    }
}

struct CountingReader<R: Read> {
    inner: R,
    count_fast: usize,
    count_count: usize,
    count_slow: Arc<Mutex<usize>>,
}
impl<R: Read> CountingReader<R> {
    fn increase(&mut self, increase: usize) {
        self.count_fast += increase;
        self.count_count += 1;
        if self.count_count % 25 == 0 {
            self.count_count = 0;
            let mut count_slow = self.count_slow.lock().unwrap();
            *count_slow = self.count_fast
        }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let r = self.inner.read(buf);
        match r {
            Ok(b) => {
                self.increase(b);
                Ok(b)
            }
            other => other,
        }
    }
}
impl<R: Read> CountingReader<R> {
    pub fn new(reader: R) -> (Self, Arc<Mutex<usize>>) {
        let count = Arc::new(Mutex::new(0));
        (
            Self {
                count_slow: count.clone(),
                inner: reader,
                count_fast: 0,
                count_count: 0,
            },
            count,
        )
    }
}

fn nice_print(addresses: usize, entities: usize, bytes: usize, start: &Instant) {
    info!(
        "Processed {} addresses; {} entities in total; {} of input processed in {}s; {}/s in avg.",
        addresses,
        entities,
        human_bytes::human_bytes(bytes as f64),
        start.elapsed().as_secs(),
        human_bytes::human_bytes(bytes as f64 / start.elapsed().as_secs_f64()),
    );
}

fn process_tags(tags: Tags) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let co = tags.get("addr:country").map(|v| v.to_string());
    let ci = tags.get("addr:city");
    let po = tags.get("addr:postcode");
    let st = tags.get("addr:street");
    let hn = tags.get("addr:housenumber");

    match (co, ci, po, st, hn) {
        (None, None, None, None, None) => None,
        (co, ci, po, st, hn) => {
            let address = IncompleteAddress {
                housenumber: hn.map(|s| s.to_string()),
                postcode: po.map(|s| s.to_string()),
                city: ci.map(|s| s.to_string()),
                street: st.map(|s| s.to_string()),
                country: co.map(|s| s.to_string()),
            };
            if is_unfixable(&address) {
                None
            } else {
                Some(serde_json::to_string(&address).unwrap())
            }
        }
    }
}

pub fn stdin_stdout_database(input: PathBuf) -> Result<(), String> {
    let reading = OpenOptions::new()
        .read(true)
        .create(false)
        .open(input)
        .map_err(|e| e.to_string())?;
    let mut stdout = io::stdout().lock();
    let (reader, bytes_read) = CountingReader::new(reading);

    let mut pbf = osmpbfreader::OsmPbfReader::new(reader);
    let mut counter_addresses = 0;
    let mut counter_entities = 0;
    let start = Instant::now();
    for obj in pbf.par_iter() {
        let obj = obj.map_err(|e| format!("{:?}", e))?;

        let way_node_rel = match &obj {
            &OsmObj::Way(_) => "way ",
            &OsmObj::Relation(_) => "rel ",
            &OsmObj::Node(_) => "nod ",
        };

        let tags = match obj {
            OsmObj::Way(w) => w.tags,
            OsmObj::Node(n) => n.tags,
            OsmObj::Relation(r) => r.tags,
        };
        counter_entities += 1;
        match process_tags(tags) {
            None => (),
            Some(t) => {
                stdout
                    .write_all(way_node_rel.as_bytes())
                    .expect("Write stdout");
                stdout
                    .write_all(t.as_bytes())
                    .expect("Error writing to stdout");
                stdout
                    .write_all("\n".as_bytes())
                    .expect("Error writing to stdout");
                counter_addresses += 1;
                if counter_addresses % 1000 == 0 {
                    nice_print(
                        counter_addresses,
                        counter_entities,
                        *bytes_read.lock().unwrap(),
                        &start,
                    );
                }
            }
        }
    }
    nice_print(
        counter_addresses,
        counter_entities,
        *bytes_read.lock().unwrap(),
        &start,
    );
    Ok(())
}
