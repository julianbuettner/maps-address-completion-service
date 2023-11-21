use multimap::MultiMap;
use std::{
    cmp::max,
    collections::{BTreeMap, HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{self, BufReader, Read, Seek, Write},
    path::PathBuf,
    rc::Rc,
    sync::{
        mpsc::{channel, Receiver},
        Arc, Mutex,
    },
    thread::spawn,
    time::{Duration, Instant},
    vec,
};
use swapvec::{Compression, SwapVec};

use log::{error, info};
use num_format::{Locale, ToFormattedString};
use osmpbfreader::{
    reader::ObjAndDeps, Node, NodeId, OsmId, OsmObj, OsmPbfReader, Relation, Tags, Way,
};
use serde::{Deserialize, Serialize};
use smartstring::{LazyCompact, SmartString};

use crate::verbose_reader::VerboseReader;

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

fn vec_to_btree(v: &Vec<Rc<ObjAndDeps>>) -> BTreeMap<OsmId, OsmObj> {
    let mut t = BTreeMap::new();
    for item in v {
        t.insert(item.inner.id(), item.inner.clone());
        let mut tt = vec_to_btree(&item.deps);
        t.append(&mut tt);
    }
    t
}

fn avg_coords(it: impl Iterator<Item = (i32, i32)>) -> (i32, i32) {
    let (mut a, mut b) = (0, 0);
    let mut count = 0;
    for (aa, bb) in it {
        count += 1;
        a += aa as i64;
        b += bb as i64;
    }
    ((a / count) as i32, (b / count) as i32)
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

fn node_to_address(
    Node {
        id: _,
        tags,
        decimicro_lat,
        decimicro_lon,
    }: Node,
) -> Result<IncompleteAddressCoord, String> {
    IncompleteAddressCoord::from_tags_and_coords(tags, decimicro_lon, decimicro_lat)
        .ok_or(format!("TODO"))
}

fn way_to_cordinates(
    Way { id, tags: _, nodes }: Way,
    tree: &BTreeMap<OsmId, OsmObj>,
) -> Result<(i32, i32), String> {
    let coordinates: Result<Vec<(i32, i32)>, String> = nodes
        .iter()
        .map(|node_id| tree.get(&OsmId::Node(node_id.clone())))
        .map(|node| match node {
            None => Err(format!("Way {} references to not existing node id.", id.0)),
            Some(OsmObj::Way(w)) => Err(format!(
                "Way {} references to way {}. Not a node.",
                id.0, w.id.0
            )),
            Some(OsmObj::Relation(r)) => Err(format!(
                "Way {} references to relation {}. Not a node.",
                id.0, r.id.0
            )),
            Some(OsmObj::Node(n)) => Ok((n.decimicro_lon, n.decimicro_lat)),
        })
        .collect();
    Ok(avg_coords(coordinates?.into_iter()))
}

fn way_to_address(
    way: Way,
    tree: &BTreeMap<OsmId, OsmObj>,
) -> Result<IncompleteAddressCoord, String> {
    let (long, lat) = way_to_cordinates(way.clone(), tree)?;
    IncompleteAddressCoord::from_tags_and_coords(way.tags, long, lat)
        .ok_or("Missing housenumber or street".into())
}

fn relation_to_coordinates(
    relation: Relation,
    tree: &BTreeMap<OsmId, OsmObj>,
) -> Result<(i32, i32), String> {
    let result: Result<Vec<(i32, i32)>, String> = relation
        .refs
        .into_iter()
        .map(|id| match tree.get(&id.member) {
            None => Err(format!("TODO")),
            Some(OsmObj::Node(node)) => Ok((node.decimicro_lon, node.decimicro_lat)),
            Some(OsmObj::Relation(rel)) => relation_to_coordinates(rel.clone(), tree),
            Some(OsmObj::Way(way)) => way_to_cordinates(way.clone(), tree),
        })
        .collect();
    Ok(avg_coords(result?.into_iter()))
}

fn relation_to_address(
    relation: Relation,
    tree: &BTreeMap<OsmId, OsmObj>,
) -> Result<IncompleteAddressCoord, String> {
    let (long, lat) = relation_to_coordinates(relation.clone(), tree)?;
    IncompleteAddressCoord::from_tags_and_coords(relation.tags, long, lat)
        .ok_or("Missing street or housenumber".into())
}

fn reader_from_path_buf(path: PathBuf) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .create(false)
        .open(path)
        .map_err(|e| e.to_string())
}

fn output_items(elements: Receiver<(OsmObj, BTreeMap<OsmId, OsmObj>)>) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    let mut count = 0;
    while let Ok((obj, deps)) = elements.recv() {
        let tags = obj.tags();
        let addr = match obj {
            OsmObj::Node(node) => node_to_address(node.clone())?,
            OsmObj::Way(way) => way_to_address(way.clone(), &deps)?,
            OsmObj::Relation(rel) => relation_to_address(rel.clone(), &deps)?,
        };
        serde_json::to_writer(&mut stdout, &addr).map_err(|e| e.to_string())?;
        stdout
            .write_all("\n".as_bytes())
            .map_err(|e| e.to_string())?;
        if count % 10_000 == 0 {
            info!("{}K addresses processed", count / 1000);
        }
        count += 1;
    }
    Ok(())
}

pub fn process_osm_pdf_to_stdout(input: PathBuf, memory_gib: f32) -> Result<(), String> {
    let memory_gib = max(memory_gib, 0.1);
    // let (reader, bytes_read) = CountingReader::new(reader_from_path_buf(input.clone())?);
    let file = reader_from_path_buf(input.clone())?;
    let meta = file
        .metadata()
        .map_err(|e| format!("Could not read file size: {}", e.to_string()))?;
    let (reader, reader_manager) = VerboseReader::new(file, meta.len() as usize);

    let mut pbf = osmpbfreader::OsmPbfReader::new(reader);
    info!(
        "Read file \"{}\" multiple times to resolve all relations between entries.",
        input.as_os_str().to_str().unwrap_or("<invalid path>")
    );
    info!("This might take multiple minutes...");
    let reader_manager = reader_manager.print_interval(Duration::from_secs(3));
    reader_manager.start_printing();
    let (sender, recv) = channel();

    let output_thread = spawn(|| output_items(recv));

    pbf.get_objs_and_deps_on_the_fly(
        |obj| is_address(obj.tags()),
        |item| sender.send((item.inner, vec_to_btree(&item.deps))).unwrap(),
        // 12 GiB for 6M objects
        // 2 GiB for 1M objects
        // 1 GiB for 500K objects
        (memory_gib * 500_000.) as usize
    );
    reader_manager.stop_printing();

    info!("Join stdout thread...");
    drop(sender);
    let _ = output_thread.join().map_err(|_| "Error joining thread".to_string())?;
    info!("Joined.");

    Ok(())
}
