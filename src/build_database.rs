use std::{
    io::{self, Read, Write},
    sync::{Arc, Mutex},
    time::Instant,
};

use osmpbfreader::OsmObj;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Address {
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

fn nice_print(addresses: usize, nodes: usize, bytes: usize, start: &Instant) {
    eprintln!(
        "Processed {} complete addresses; {} nodes in total; {} of input processed in {}s; {}/s in avg.",
        addresses,
        nodes,
        human_bytes::human_bytes(bytes as f64),
        start.elapsed().as_secs(),
        human_bytes::human_bytes(bytes as f64 / start.elapsed().as_secs_f64()),
    );
}

pub fn stdin_stdout_database() -> Result<(), String> {
    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let (reader, bytes_read) = CountingReader::new(stdin);

    let mut pbf = osmpbfreader::OsmPbfReader::new(reader);
    let mut counter_addresses = 0;
    let mut counter_nodes = 0;
    let start = Instant::now();
    for obj in pbf.par_iter() {
        let obj = obj.map_err(|e| format!("{:?}", e))?;

        if let OsmObj::Node(n) = obj {
            counter_nodes += 1;
            if n.tags.is_empty() {
                continue;
            }
            let co = n.tags.get("addr:country");
            let ci = n.tags.get("addr:city");
            let po = n.tags.get("addr:postcode");
            let st = n.tags.get("addr:street");
            let hn = n.tags.get("addr:housenumber");

            match (co, ci, po, st, hn) {
                (Some(co), Some(ci), Some(po), Some(st), Some(hn)) => {
                    let address = Address {
                        housenumber: hn.to_string(),
                        postcode: po.to_string(),
                        city: ci.to_string(),
                        street: st.to_string(),
                        country: co.to_string(),
                    };
                    let text = format!("{}\n", serde_json::to_string(&address).unwrap());
                    counter_addresses += 1;
                    stdout
                        .write_all(text.as_bytes())
                        .map_err(|e| e.to_string())?;
                }
                _ => continue,
            }
            if counter_addresses % 1000 == 0 {
                nice_print(
                    counter_addresses,
                    counter_nodes,
                    *bytes_read.lock().unwrap(),
                    &start,
                );
            }
        }
    }
    nice_print(
        counter_addresses,
        counter_nodes,
        *bytes_read.lock().unwrap(),
        &start,
    );
    Ok(())
}
