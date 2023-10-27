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

use crate::sorted_vec::SortedVec;

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
    bytes: usize,
    start: &Instant,
) {
    info!(
        "Pass 2: Processed {} node coordinates; found {} ways, {} entities in this pass; \
        {} of input processed in {}s, {}/s in avg.",
        required_node_ids_collected,
        ways_matched,
        entities,
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
                address_way_count += 1;
                for node_id in way.nodes.into_iter() {
                    node_ids_required_by_ways.insert(node_id.0);
                }
            }
            OsmObj::Node(node) => {
                address_node_count += 1;
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
                out_inc_addr_coord(out, &inc)?;
            }
        }
    }
    Ok(node_ids_required_by_ways)
}

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

    for obj in pbf.par_iter() {
        entity_count += 1;
        if entity_count % 4_000_000 == 0 {
            nice_print_pass_two(
                entity_count,
                required_node_ids_collected,
                ways_matched,
                *bytes_read.lock().unwrap(),
                start,
            )
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
            OsmObj::Relation(_) => (),
            OsmObj::Way(way) => {
                let node_coordinates = way
                    .nodes
                    .iter()
                    .map(|NodeId(node_id)| node_coordinates.get(node_id).map(|(a, b)| (*a, *b)));
                let average_coordinates = avg_coords(node_coordinates);

                if average_coordinates.is_none() {
                    return Err(format!(
                        "Way {} requires nodes which have not been read yet",
                        way.id.0
                    ));
                }
                ways_matched += 1;
                let (decimicro_lat, decimicro_lon) = average_coordinates.unwrap();
                let tags = way.tags;
                let inc = IncompleteAddressCoord {
                    housenumber: tags.get("addr:housenumber").cloned().unwrap(),
                    street: tags.get("addr:street").cloned().unwrap(),
                    zip: tags.get("addr:postcode").cloned(),
                    city: tags.get("addr:postcode").cloned(),
                    country: tags.get("addr:postcode").cloned(),
                    lat: decimicro_lat,
                    long: decimicro_lon,
                };
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
