use multimap::MultiMap;
use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{self, BufReader, Read, Seek, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use log::{error, info};
use num_format::{Locale, ToFormattedString};
use osmpbfreader::{Node, NodeId, OsmObj, OsmPbfReader, Tags, Way};
use serde::{Deserialize, Serialize};
use smartstring::{LazyCompact, SmartString};

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
    pub fn from_tags_and_coords(t: Tags, long: i32, lat: i32) -> Option<Self> {
        let inc = IncompleteAddress::from_tags(t)?;
        Some(Self::from_incomplete_address_and_coords(inc, long, lat))
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

struct CountingReader<R: Read> {
    inner: BufReader<R>,
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
        // Do not reset counters
        // self.count_fast = 0;
        // self.count_count = 0;
        self.inner.seek(pos)
    }
}

impl<R: Read> CountingReader<R> {
    pub fn new(reader: R) -> (Self, Arc<Mutex<usize>>) {
        let count = Arc::new(Mutex::new(0));
        (
            Self {
                count_slow: count.clone(),
                inner: BufReader::new(reader),
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
        "Pass 1: Processed {} node addresses; {} address ways; {} entities in total, \
        {} of input processed in {}s; {}/s in avg.",
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

fn nice_print_pass_two(
    entities: usize,
    required_node_ids_collected: usize,
    ways_matched: usize,
    way_backlog: usize,
    bytes: usize,
    start: &Instant,
) {
    info!(
        "Pass 2: Processed {} node coordinates; found {} ways, {} entities in this pass; \
        {} ways in backlog; {} of input processed in {}s, {}/s in avg.",
        required_node_ids_collected,
        ways_matched,
        entities,
        way_backlog,
        human_bytes::human_bytes(bytes as f64),
        start.elapsed().as_secs(),
        human_bytes::human_bytes(bytes as f64 / start.elapsed().as_secs_f64()),
    );
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

pub fn pass_one<R: Read>(
    pbf: &mut OsmPbfReader<R>,
    bytes_read: &Mutex<usize>,
    start: &Instant,
    out: &mut impl Write,
) -> Result<HashSet<i64>, String> {
    // output all nodes, collect node ids required for ways
    let mut node_ids_required_by_ways = HashSet::new();
    let mut address_node_count: usize = 0;
    let mut address_way_count: usize = 0;
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
                if !is_address(&way.tags) {
                    continue;
                }
                address_way_count += 1;
                for node_id in way.nodes.into_iter() {
                    node_ids_required_by_ways.insert(node_id.0);
                }
            }
            OsmObj::Node(node) => {
                address_node_count += 1;
                let inc = IncompleteAddressCoord::from_incomplete_address_and_coords(
                    IncompleteAddress::from_tags(node.tags).unwrap(),
                    node.decimicro_lon,
                    node.decimicro_lat,
                );
                out_inc_addr_coord(out, &inc)?;
            }
        }
    }
    Ok(node_ids_required_by_ways)
}

// fn process_backlog(backlog: Vec<Way>, node_coordinates: HashMap<i64, (i32, i32)>) -> Result<Vec<Way>, String> {
//     let mut new_backlog = Vec::new();
//     for way in backlog {
//     }
//     Ok(backlog)
// }

fn pass_two<R: Read + Seek>(
    pbf: &mut OsmPbfReader<R>,
    bytes_read: &Mutex<usize>,
    node_ids_required_by_ways: HashSet<i64>,
    start: &Instant,
    out: &mut impl Write,
) -> Result<(), String> {
    pbf.rewind()
        .map_err(|e| format!("Failed to rewind osm.pbf file: {}", e.to_string()))?;
    let mut entity_count: usize = 0;
    let mut required_node_ids_collected: usize = 0;
    let mut ways_matched: usize = 0;
    let mut node_coordinates: HashMap<i64, (i32, i32)> = HashMap::new();
    let mut way_backlog: Vec<Way> = Vec::new();

    for obj in pbf.par_iter() {
        entity_count += 1;
        if entity_count % 4_000_000 == 0 {
            nice_print_pass_two(
                entity_count,
                required_node_ids_collected,
                ways_matched,
                way_backlog.len(),
                *bytes_read.lock().unwrap(),
                start,
            )
        }
        let obj = obj.map_err(|e| format!("{:?}", e))?;
        match obj {
            OsmObj::Relation(_) => (),
            OsmObj::Way(way) => {
                if !is_address(&way.tags) {
                    continue;
                }
                let node_coordinates = way
                    .nodes
                    .iter()
                    .map(|NodeId(node_id)| node_coordinates.get(node_id).map(|(a, b)| (*a, *b)));
                let average_coordinates = avg_coords(node_coordinates);

                if average_coordinates.is_none() {
                    way_backlog.push(way);
                    continue;
                }
                ways_matched += 1;
                let (decimicro_lon, decimicro_lat) = average_coordinates.unwrap();
                let inc = IncompleteAddressCoord::from_tags_and_coords(
                    way.tags,
                    decimicro_lon,
                    decimicro_lat,
                )
                .unwrap();
                out_inc_addr_coord(out, &inc)?;
            }
            OsmObj::Node(node) => {
                if node_ids_required_by_ways.contains(&node.id.0) {
                    required_node_ids_collected += 1;
                    node_coordinates.insert(node.id.0, (node.decimicro_lon, node.decimicro_lat));
                }
            }
        }
    }
    info!("Process backlog of {} ways...", way_backlog.len());
    for way in way_backlog {
        let node_coordinates = way
            .nodes
            .iter()
            .map(|NodeId(node_id)| node_coordinates.get(node_id).map(|(a, b)| (*a, *b)));
        let average_coordinates = avg_coords(node_coordinates);
        if average_coordinates.is_none() {
            return Err(format!(
                "Way {} is missing node coordinates! Corrupt *osm.pbf?",
                way.id.0
            ));
        }
        ways_matched += 1;
        let (decimicro_lat, decimicro_lon) = average_coordinates.unwrap();
        let inc =
            IncompleteAddressCoord::from_tags_and_coords(way.tags, decimicro_lon, decimicro_lat)
                .unwrap();
        out_inc_addr_coord(out, &inc)?;
    }

    Ok(())
}

pub fn process_osm_pdf_to_stdout(input: PathBuf) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    let (reader, bytes_read) = CountingReader::new(reader_from_path_buf(input.clone())?);

    let mut pbf = osmpbfreader::OsmPbfReader::new(reader);
    let start = Instant::now();
    let node_ids_required_by_ways = pass_one(&mut pbf, &bytes_read, &start, &mut stdout)?;
    pass_two(
        &mut pbf,
        &bytes_read,
        node_ids_required_by_ways,
        &start,
        &mut stdout,
    )?;
    // info!(
    //     "Searching node coordinates for batch of ways {}...",
    //     address_ways.len()
    // );
    // let complete_address_coords = patch_address_ways(
    //     reader_from_path_buf(input.clone())?,
    //     address_ways.drain(0..),
    //     highest_node_id,
    // )?;
    // for complete in complete_address_coords {
    //     out_inc_addr_coord(&mut stdout, &complete)?;
    // }
    info!("Done");
    Ok(())
}
