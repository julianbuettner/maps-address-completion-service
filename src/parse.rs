use std::{
    io::{self, Read, Write},
    sync::{Arc, Mutex},
    time::Instant,
};

use log::info;
use osmpbfreader::{OsmObj, Tags};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Address {
    pub country: String,
    pub city: String,
    pub postcode: String,
    pub street: String,
    pub housenumber: String,
}

struct CountingReader<R: Read> {
    inner: R,
    count: Arc<Mutex<usize>>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let r = self.inner.read(buf);
        match r {
            Ok(b) => {
                let mut v = self.count.lock().unwrap();
                *v += b;
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
                count: count.clone(),
                inner: reader,
            },
            count,
        )
    }
}

fn nice_print(addresses: usize, entities: usize, bytes: usize, start: &Instant) {
    info!(
        "Processed {} complete addresses; {} entities in total; {} of input processed in {}s; {}/s in avg.",
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
        (Some(co), Some(ci), Some(po), Some(st), Some(hn)) => {
            let address = Address {
                housenumber: hn.to_string(),
                postcode: po.to_string(),
                city: ci.to_string(),
                street: st.to_string(),
                country: co.to_string(),
            };
            Some(format!("{}\n", serde_json::to_string(&address).unwrap()))
        }
        _ => None,
    }
}

pub fn stdin_stdout_database() -> Result<(), String> {
    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let (reader, bytes_read) = CountingReader::new(stdin);

    let mut pbf = osmpbfreader::OsmPbfReader::new(reader);
    let mut counter_addresses = 0;
    let mut counter_entities = 0;
    let start = Instant::now();
    for obj in pbf.par_iter() {
        let obj = obj.map_err(|e| format!("{:?}", e))?;

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
                    .write_all(t.as_bytes())
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
