use std::{
    collections::HashSet,
    io::{self, BufRead, BufReader, Read}, cmp::Ordering,
};

use serde::{Serialize, Deserialize};

use crate::{parse::Address, sorted_vec::SortedVec};

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

fn num_compressable(a: &str) -> bool {
    match a.parse::<u16>() {
        Err(_) => false,
        Ok(i) => a == i.to_string().as_str(),
    }
}

#[derive(Serialize, Deserialize)]
pub enum Housenumber {
    Compressed(u16),
    Index(u32),
}

#[derive(Serialize, Deserialize)]
pub struct Street {
    index: u32,
    housenumbers: Vec<Housenumber>,
}

#[derive(Serialize, Deserialize)]
pub struct PostalArea {
    code: String,
    streets: Vec<Street>,
}

#[derive(Serialize, Deserialize)]
pub struct City {
    name: String,
    areas: Vec<PostalArea>,
}

#[derive(Serialize, Deserialize)]
pub struct Country {
    code: String,
    cities: Vec<City>,
}

#[derive(Serialize, Deserialize)]
pub struct World {
    unique_streets: SortedVec<String>,
    housenumbers: SortedVec<String>,
    countries: Vec<Country>,
}

impl Street {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            housenumbers: Vec::new(),
        }
    }
    pub fn insert_housenumber(&mut self, hn: Housenumber) {
        self.housenumbers.push(hn);
    }
    pub fn sort_with(&mut self, hn_sort: impl FnMut(&Housenumber, &Housenumber) -> Ordering) {
        self.housenumbers.sort_by(hn_sort)
    }
}

impl PostalArea {
    pub fn new(code: String) -> Self {
        Self {
            code,
            streets: Vec::new(),
        }
    }
    pub fn insert_address(&mut self, street_index: u32, hn: Housenumber) {
        let street_mut = self.streets.iter_mut().find(|e| e.index == street_index);
        if let Some(street) = street_mut {
            street.insert_housenumber(hn);
        } else {
            let mut street = Street::new(street_index);
            street.insert_housenumber(hn);
            self.streets.push(street);
        }
    }
    pub fn sort_with(&mut self, mut hn_sort: impl FnMut(&Housenumber, &Housenumber) -> Ordering) {
        self.streets.sort_by(|a, b| a.index.cmp(&b.index));
        for street in self.streets.iter_mut() {
            street.sort_with(&mut hn_sort)
        }
    }
}

impl City {
    pub fn new(name: String) -> Self {
        Self {
            name,
            areas: Vec::new(),
        }
    }
    pub fn insert_address(&mut self, postal_code: String, street_index: u32, hn: Housenumber) {
        let post_mut = self.areas.iter_mut().find(|e| e.code == postal_code);
        if let Some(area) = post_mut {
            area.insert_address(street_index, hn);
        } else {
            let mut area: PostalArea = PostalArea::new(postal_code);
            area.insert_address(street_index, hn);
            self.areas.push(area);
        }
    }
    pub fn sort_with(&mut self, mut hn_sort: impl FnMut(&Housenumber, &Housenumber) -> Ordering) {
        self.areas.sort_by(|a, b| a.code.cmp(&b.code));
        for area in self.areas.iter_mut() {
            area.sort_with(&mut hn_sort)
        }
    }
}

impl Country {
    pub fn new(code: String) -> Self {
        Self {
            code,
            cities: Vec::new(),
        }
    }
    pub fn insert_address(
        &mut self,
        city: String,
        postal_code: String,
        street_index: u32,
        hn: Housenumber,
    ) {
        let city_mut = self.cities.iter_mut().find(|e| e.name == city);
        if let Some(city) = city_mut {
            city.insert_address(postal_code, street_index, hn);
        } else {
            let mut city = City::new(city);
            city.insert_address(postal_code, street_index, hn);
            self.cities.push(city);
        }
    }
    pub fn sort_with(&mut self, mut hn_sort: impl FnMut(&Housenumber, &Housenumber) -> Ordering) {
        self.cities.sort_by(|a, b| a.name.cmp(&b.name));
        for city in self.cities.iter_mut() {
            city.sort_with(&mut hn_sort)
        }
    }
}

impl World {
    pub fn new(unique_streets: SortedVec<String>, housenumbers: SortedVec<String>) -> Self {
        Self {
            housenumbers,
            unique_streets,
            countries: Vec::new(),
        }
    }
    pub fn insert_address(
        &mut self,
        country_code: String,
        city_name: String,
        zip: String,
        street: String,
        housenumber: String,
    ) {
        let housenumber = match num_compressable(&housenumber) {
            true => Housenumber::Compressed(housenumber.parse().unwrap()),
            false => Housenumber::Index(
                self.housenumbers
                    .index_of(&housenumber)
                    .expect("self.housenumbers did not contain inserted house number")
                    as u32,
            ),
        };
        let street_index = self
            .unique_streets
            .index_of(&street)
            .expect("self.unique_streets did no contain inserted street name")
            as u32;
        let country_mut = self.countries.iter_mut().find(|e| e.code == country_code);
        if let Some(country) = country_mut {
            country.insert_address(city_name, zip, street_index, housenumber);
        }
    }
    pub fn sort(&mut self) {
        self.countries.sort_by(|a, b| a.code.cmp(&b.code));
        for country in self.countries.iter_mut() {
            country.sort_with(|hn_a: &Housenumber, hn_b: &Housenumber| {
                let a = match hn_a {
                    Housenumber::Compressed(v) => v.to_string(),
                    Housenumber::Index(i) => self.housenumbers.get(*i as usize).expect("Housenumber index greater then housenumber list length").to_string(),
                };
                let b = match hn_b {
                    Housenumber::Compressed(v) => v.to_string(),
                    Housenumber::Index(i) => self.housenumbers.get(*i as usize).expect("Housenumber index greater then housenumber list length").to_string(),
                };
                a.cmp(&b)
            })
        }
    }
}

fn compress(streets: SortedVec<String>, hn: SortedVec<String>, addresses: Vec<Address>) {
    let mut world = World::new(streets, hn);
    let len = addresses.len();
    for (i, addr) in addresses.into_iter().enumerate() {
        if i % 1000 == 0 {
            eprintln!("Storing addr {}/{}", i, len)
        }
        world.insert_address(addr.country, addr.city, addr.postcode, addr.street, addr.housenumber);
    }
    eprintln!("Sorting every wolrd entry...");
    world.sort();
    eprintln!("Done. Dumping to stdout...");
    let stdout = io::stdout().lock();
    bincode::serialize_into(stdout, &world).unwrap();
    eprintln!("Done!");
}

pub fn read_and_compress() -> Result<(), String> {
    eprintln!("Reading jsonl from stdin...");
    let stdin = io::stdin().lock();
    let mut addresses: Vec<Address> = Vec::new();
    let mut streets: HashSet<String> = HashSet::new();
    let mut uncompressable_house_numbers: HashSet<String> = HashSet::new();
    for (i, item) in iter_items(stdin).enumerate() {
        if i % 100000 == 0 {
            eprintln!(
                "Processed {} addresses, {} unique street names, {} unique uncompressable house numbers",
                  i, streets.len(), uncompressable_house_numbers.len());
        }
        let item = item?;
        streets.insert(item.street.clone());
        if !num_compressable(item.housenumber.as_str()) {
            uncompressable_house_numbers.insert(item.housenumber.clone());
        }
        addresses.push(item);
    }

    eprintln!("Sort streets ({})...", streets.len());
    let streets_sorted: SortedVec<String> = streets.into_iter().collect::<Vec<_>>().into();
    eprintln!(
        "Sort house numbers ({})...",
        uncompressable_house_numbers.len()
    );
    let housenumbers_sorted: SortedVec<String> = uncompressable_house_numbers
        .into_iter()
        .collect::<Vec<String>>()
        .into();

    eprintln!(
        "Processed {} addresses, {} unique street names, {} unique uncompressable house numbers",
        addresses.len(),
        streets_sorted.len(),
        housenumbers_sorted.len()
    );

    compress(streets_sorted, housenumbers_sorted, addresses);

    Ok(())
}
