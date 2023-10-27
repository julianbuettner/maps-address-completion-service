use multimap::MultiMap;
use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{self, BufReader, Read, Seek, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant, cmp::max,
};

use log::{error, info};
use num_format::{Locale, ToFormattedString};
use osmpbfreader::{Node, NodeId, OsmObj, OsmPbfReader, Tags, Way};
use serde::{Deserialize, Serialize};
use smartstring::{LazyCompact, SmartString};

const ADDRESS_WAYS_BATCH_SIZE: usize = 4_000_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct IncompleteAddressCoord {
    pub country: Option<SmartString<LazyCompact>>,
    pub city: Option<SmartString<LazyCompact>>,
    pub zip: Option<SmartString<LazyCompact>>,
    pub street: SmartString<LazyCompact>,
    pub housenumber: SmartString<LazyCompact>,
    pub long: i32,
    pub lat: i32,
}

impl IncompleteAddressCoord {
    pub fn from_incomplete_address_and_coords(inc: IncompleteAddress, long: i32, lat: i32) -> Self {
        Self {
            country: inc.country,
            city: inc.city,
            housenumber: inc.housenumber,
            zip: inc.zip,
            street: inc.street,
            long,
            lat,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncompleteAddress {
    pub country: Option<SmartString<LazyCompact>>,
    pub city: Option<SmartString<LazyCompact>>,
    pub zip: Option<SmartString<LazyCompact>>,
    pub street: SmartString<LazyCompact>,
    pub housenumber: SmartString<LazyCompact>,
}

impl IncompleteAddress {
    pub fn from_tags(t: Tags) -> Option<Self> {
        Some(Self {
            country: t.get("addr:country").cloned(),
            city: t.get("addr:city").cloned(),
            zip: t.get("addr:postcode").cloned(),
            street: t.get("addr:street").cloned()?,
            housenumber: t.get("addr:housenumber").cloned()?,
        })
    }
}

fn avg_coords(it: impl Iterator<Item = Option<(i32, i32)>>) -> Option<(i32, i32)> {
    let (mut a, mut b) = (0, 0);
    let mut count = 0;
    for item in it {
        count += 1;
        let (aa, bb) = item?;
        a += aa as i64;
        b += bb as i64;
    }
    Some(((a / count) as i32, (b / count) as i32))
}

struct IncompleteWay {
    pub addr: IncompleteAddress,
    pub node_ids: Vec<NodeId>,
    pub coords: Vec<Option<(i32, i32)>>,
}

impl IncompleteWay {
    pub fn new(way: Way) -> Self {
        let mut node_ids = way.nodes;
        node_ids.sort();
        node_ids.dedup();
        Self {
            coords: vec![None; node_ids.len()],
            addr: IncompleteAddress::from_tags(way.tags).unwrap(),
            node_ids,
        }
    }
    pub fn min_unapplied(&self) -> Option<i64> {
        self.coords
            .iter()
            .zip(self.node_ids.iter())
            .filter(|(coord, _id)| coord.is_none())
            .map(|(_coord, id)| id.0)
            .min()
    }
    pub fn apply(&mut self, node: Node) -> Result<(), ()> {
        let index = self
            .node_ids
            .iter()
            .position(|f| f.0 == node.id.0)
            .ok_or(())?;
        self.coords[index] = Some((node.decimicro_lon, node.decimicro_lat));
        Ok(())
    }
    pub fn to_incomplete_address_coord(self) -> Result<IncompleteAddressCoord, Self> {
        let coords = avg_coords(self.coords.iter().copied());
        match coords {
            None => Err(self),
            Some((long, lat)) => Ok(IncompleteAddressCoord::from_incomplete_address_and_coords(
                self.addr, long, lat,
            )),
        }
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
        if self.count_count % 250 == 0 {
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

impl<R: Seek + Read> Seek for CountingReader<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.count_fast = 0;
        self.count_count = 0;
        self.inner.seek(pos)
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

fn is_address(t: &Tags) -> bool {
    t.get("addr:housenumber").is_some() && t.get("addr:street").is_some()
}

fn nice_print_pass_one(
    entities: usize,
    way_count: usize,
    node_address_count: usize,
    bytes: usize,
    start: &Instant,
) {
    info!(
        "Processed {} node addresses; {} address ways; {} entities in total, {} of input processed in {}s; {}/s in avg.",
        node_address_count.to_formatted_string(&Locale::fr),
        way_count.to_formatted_string(&Locale::fr),
        entities.to_formatted_string(&Locale::fr),
        human_bytes::human_bytes(bytes as f64),
        start.elapsed().as_secs(),
        human_bytes::human_bytes(bytes as f64 / start.elapsed().as_secs_f64()),
    )
}

fn reader_from_path_buf(path: PathBuf) -> Result<impl Read + Seek, String> {
    let reading = OpenOptions::new()
        .read(true)
        .create(false)
        .open(path)
        .map_err(|e| e.to_string())?;
    Ok(BufReader::new(reading))
}

fn nice_print_way_matching(elems_left: usize, coords_found: usize, bytes: usize) {
    info!(
        "Matching ways' node points, {} ways to go, {} coordinates found, parsed {} in search of nodes",
        elems_left,
        coords_found,
        human_bytes::human_bytes(bytes as f64)
    );
}

fn patch_address_ways<T: Read + Seek>(
    reader: T,
    address_ways: impl IntoIterator<Item = IncompleteWay>,
    highest_node_id: i64,
) -> Result<Vec<IncompleteAddressCoord>, String> {
    let (reader, bytes_read) = CountingReader::new(reader);
    let mut pbf_reader = OsmPbfReader::new(reader);
    pbf_reader.rewind().map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    // Houses / shapes share nodes, so we need a multimap
    let mut map: MultiMap<i64, IncompleteWay> = address_ways
        .into_iter()
        .map(|v| (v.min_unapplied().unwrap_or(i64::MAX), v))
        .collect();

    let mut last_node_id = i64::MIN;
    let mut coords_found = 0;

    for item in pbf_reader.par_iter() {
        let item = item.map_err(|e| e.to_string())?;

        let OsmObj::Node(n) = item else {
            continue;
        };
        if n.id.0 <= last_node_id {
            return Err(format!(
                "Node {} followed node {}. Expect osm.pbf nodes to be sorted by id (ascending).",
                n.id.0, last_node_id
            ));
        }
        let current_node_id = n.id.0;
        last_node_id = current_node_id;
        if current_node_id > highest_node_id {
            // There are no more nodes in this file. Stop.
            // break;
        }

        let inc = map.remove(&n.id.0);
        let Some(incomplete_ways) = inc else {
            continue;
        };
        for mut incomplete_way in incomplete_ways.into_iter() {
            coords_found += 1;
            incomplete_way.apply(n.clone()).expect("Should have contained key");
            let new_id = incomplete_way.min_unapplied().unwrap_or(i64::MAX);
            assert!(new_id > current_node_id);
            if coords_found % 1000 == 0 {
                nice_print_way_matching(map.len(), coords_found, *bytes_read.lock().unwrap());
            }
            match incomplete_way.to_incomplete_address_coord() {
                Err(incomplete_way) => {
                    map.insert(new_id, incomplete_way);
                }
                Ok(complete_way) => result.push(complete_way),
            }
        }
    }

    if map.len() > 0 {
        error!(
            "First missing node: {}",
            map.iter().map(|(key, _value)| key).min().unwrap()
        );
        return Err(format!(
            "Failure trying to find nodes for ways. There are {} ways without all nodes matching. \
            This is likely due to a corrupt *.osm.pbf file.",
            map.len()
        ));
    }

    Ok(result)
}

fn out_inc_addr_coord(
    writer: &mut impl Write,
    elem: &IncompleteAddressCoord,
) -> Result<(), String> {
    writer
        .write_all(&serde_json::to_string(&elem).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;
    writer
        .write_all(&"\n".as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn process_osm_pdf_to_stdout(input: PathBuf, address_ways_batch_size: usize) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    let (reader, bytes_read) = CountingReader::new(reader_from_path_buf(input.clone())?);

    let mut pbf = osmpbfreader::OsmPbfReader::new(reader);

    let mut address_ways: Vec<IncompleteWay> = Vec::new();
    let mut address_node_count: usize = 0;
    let mut address_way_count: usize = 0;
    let mut highest_node_id: i64 = i64::MIN;
    let start = Instant::now();
    let mut entity_count: usize = 0;

    for obj in pbf.par_iter() {
        entity_count += 1;
        if entity_count % 4_000_000 == 0 {
            nice_print_pass_one(
                entity_count,
                address_way_count,
                address_node_count,
                *bytes_read.lock().unwrap(),
                &start,
            );
        }

        let obj = obj.map_err(|e| format!("{:?}", e))?;
        let tags = match &obj {
            OsmObj::Way(w) => &w.tags,
            OsmObj::Node(n) => &n.tags,
            OsmObj::Relation(r) => &r.tags,
        };
        if !is_address(&tags) {
            continue;
        }
        match obj {
            OsmObj::Relation(_) => (), // ignore relations
            OsmObj::Way(way) => {
                address_way_count += 1;
                address_ways.push(IncompleteWay::new(way));
                if address_ways.len() >= address_ways_batch_size {
                    info!(
                        "Searching node coordinates for batch of ways {}...",
                        address_ways.len()
                    );
                    let complete_address_coords = patch_address_ways(
                        reader_from_path_buf(input.clone())?,
                        address_ways.drain(0..),
                        // ASSUMPTION file contains all node ids required for a way
                        // before cointaining the way
                        highest_node_id,
                    )?;
                    for complete in complete_address_coords {
                        out_inc_addr_coord(&mut stdout, &complete)?;
                    }
                }
            }
            OsmObj::Node(node) => {
                address_node_count += 1;
                highest_node_id = max(highest_node_id, node.id.0);
                let tags = node.tags;
                let inc = IncompleteAddressCoord {
                    housenumber: tags.get("addr:housenumber").cloned().unwrap(),
                    street: tags.get("addr:street").cloned().unwrap(),
                    zip: tags.get("addr:postcode").cloned(),
                    city: tags.get("addr:postcode").cloned(),
                    country: tags.get("addr:postcode").cloned(),
                    lat: node.decimicro_lat,
                    long: node.decimicro_lon,
                };
                out_inc_addr_coord(&mut stdout, &inc)?;
            }
        }
    }

    info!(
        "Searching node coordinates for batch of ways {}...",
        address_ways.len()
    );
    let complete_address_coords = patch_address_ways(
        reader_from_path_buf(input.clone())?,
        address_ways.drain(0..),
        highest_node_id,
    )?;
    for complete in complete_address_coords {
        out_inc_addr_coord(&mut stdout, &complete)?;
    }
    info!("Done");
    Ok(())
}
