use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::Hasher,
    io::{self, BufRead, BufReader, Read},
    thread::sleep,
    time::Duration,
};

use crate::{build_database::Address, sorted_vec::SortedVec};

pub fn iter_items(io: impl Read) -> impl Iterator<Item = Result<Address, String>> {
    let buf_reader = BufReader::new(io);
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

// 99.99453% of all European house numbers
// are 15 bytes or shorter.
// std::mem::size_of<_>() == 16
pub enum Housenumber {
    Stack((u8, [u8; 14])),
    Heap(Box<Vec<u8>>),
}

impl From<String> for Housenumber {
    fn from(value: String) -> Self {
        if value.len() <= 14 {
            let mut c = [0; 14];
            for (i, v) in value.as_bytes().iter().enumerate() {
                c[i] = *v;
            }
            Housenumber::Stack((value.len() as u8, c))
        } else {
            Housenumber::Heap(Box::new(value.as_bytes().to_vec()))
        }
    }
}

type Housenumbers = Vec<String>;
type StreetId = u64;

struct Streets {
    streets: Vec<(StreetId, Housenumbers)>,
}

struct PostcalCodes {
    // ZIP, Streets
    codes: Vec<(String, Streets)>,
}

struct Cities {
    // City, ZIPs
    cities: Vec<(String, PostcalCodes)>,
}

pub struct World {
    // Copuntried, Cities
    countries: Vec<(String, Cities)>,
    street_map: HashMap<u64, String>,
}

impl Streets {
    pub fn new() -> Self {
        Self {
            streets: Vec::new(),
        }
    }
    pub fn insert_address(&mut self, street: u64, housenumber: String) {
        let (_street, housenumbers) = match self.streets.binary_search_by(|p| p.0.cmp(&street)) {
            Ok(i) => self.streets.get_mut(i).unwrap(),
            Err(i) => {
                let housenumbers = Housenumbers::new();
                self.streets.insert(i, (street, housenumbers));
                self.streets.get_mut(i).unwrap()
            }
        };
        housenumbers.push(housenumber);
    }
    pub fn shrink(&mut self) {
        self.streets.shrink_to_fit();
    }
}

impl PostcalCodes {
    pub fn new() -> Self {
        Self { codes: Vec::new() }
    }
    pub fn insert_address(&mut self, zip: String, street: u64, housenumber: String) {
        let a = match self.codes.binary_search_by(|e| e.0.cmp(&zip)) {
            Ok(i) => self.codes.get_mut(i).unwrap(),
            Err(i) => {
                let streets = Streets::new();
                self.codes.insert(i, (zip, streets));
                self.codes.get_mut(i).unwrap()
            }
        };
        a.1.insert_address(street, housenumber);
    }
    pub fn shrink(&mut self) {
        for (a, b) in self.codes.iter_mut() {
            b.shrink();
        }
        self.codes.shrink_to_fit();
    }
}

impl Cities {
    pub fn new() -> Self {
        Self { cities: Vec::new() }
    }
    pub fn insert_address(&mut self, city: String, zip: String, street: u64, housenumber: String) {
        let (_city, postal_codes) = match self.cities.binary_search_by(|item| item.0.cmp(&city)) {
            Ok(i) => self.cities.get_mut(i).unwrap(),
            Err(i) => {
                let pc = PostcalCodes::new();
                self.cities.insert(i, (city, pc));
                self.cities.get_mut(i).unwrap()
            }
        };
        postal_codes.insert_address(zip, street, housenumber)
    }
    pub fn shrink(&mut self) {
        for (a, b) in self.cities.iter_mut() {
            b.shrink();
        }
        self.cities.shrink_to_fit();
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            countries: Vec::new(),
            street_map: HashMap::new(),
        }
    }
    pub fn insert_address(&mut self, a: Address) {
        let street_hash = hash(a.street.as_str());
        self.street_map.insert(street_hash, a.street);

        let country = self.countries.iter_mut().find(|f| f.0 == a.country);
        let country_exist = match country {
            Some(c) => c,
            None => {
                let cities = Cities::new();
                self.countries.push((a.country, cities));
                self.countries.last_mut().unwrap()
            }
        };
        country_exist
            .1
            .insert_address(a.city, a.postcode, street_hash, a.housenumber);
    }
    pub fn shrink(&mut self) {
        for (a, b) in self.countries.iter_mut() {
            b.shrink();
        }
        self.countries.shrink_to_fit();
    }
}

fn hash(data: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(data.as_bytes());
    hasher.finish()
}

pub fn build() -> Result<SortedVec<String>, String> {
    eprintln!("Reading jsonl from stdin...");
    let stdin = io::stdin().lock();

    let it = iter_items(stdin);

    eprintln!("Collect addresses...");
    let mut world = World::new();
    let mut streets = SortedVec::new();
    let mut housenumber_count: HashMap<usize, usize> = HashMap::new();
    for (i, address) in it.enumerate() {
        let address = address?;
        if i % 1000 == 0 {
            eprintln!("Insert address no {} - {}", i, streets.len());
            eprintln!("Entries:{}", housenumber_count.len());
        }
        // world.insert_address(address);
        // if !streets.contains(&address.street) {
        //     println!("{}", address.street);
        //     streets.insert(address.street);
        // }
        housenumber_count.insert(
            address.housenumber.as_bytes().len(),
            housenumber_count
                .get(&address.housenumber.as_bytes().len())
                .unwrap_or(&0)
                + 1,
        );
    }

    let mut kv: Vec<(usize, usize)> = housenumber_count.into_iter().collect();
    kv.sort_by(|(a, b), (c, d)| a.cmp(c));
    for (length, count) in kv.iter() {
        println!("{}: {} times", length, count);
    }

    let total: usize = kv.iter().map(|(_, b)| b).sum();
    for i in 0..110 {
        let partition: usize = kv.iter().filter(|(a, _)| a <= &(i as usize)).map(|(_, b)| b).sum();
        println!("Count up to including {}: {} making {}% of total", i, partition, 100. * partition as f32 / total as f32);
    }

    Ok(streets)
}
