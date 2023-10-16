use std::{
    cmp::Ordering,
    collections::HashSet,
    io::{self, BufRead, BufReader, Read},
    str::FromStr,
};

use codes_iso_3166::part_1::{CountryCode, ALL_CODES};
use log::info;
use serde::{Deserialize, Serialize};

use crate::{parse::Address, sorted_vec::SortedVec};

pub fn iter_items(io: impl Read) -> impl Iterator<Item = Result<Address, String>> {
    let buf_reader = BufReader::new(io);
    buf_reader
        .lines()
        // .map(|l| l.map_err(|e| e.to_string()).map(|v| serde_json::from_str(v.as_str())))
        .map(|line| match line {
            Ok(text) => match serde_json::from_str::<Address>(text.as_str()) {
                Ok(addr) => Ok(normalize_address(addr)),
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e.to_string()),
        })
}

fn normalize_address(a: Address) -> Address {
    Address {
        country: autocorrect_country_code(a.country),
        city: a.city,
        postcode: a.postcode,
        street: a.street,
        housenumber: a.housenumber,
    }
}

fn autocorrect_country_code(c: String) -> String {
    if let Ok(_) = CountryCode::from_str(c.as_str()) {
        c
    } else {
        for code in ALL_CODES {
            if code.short_name() == c.as_str() {
                return code.to_string();
            }
            if code.local_short_name() == Some(c.as_str()) {
                return code.to_string();
            }
        }
        c
    }
}

fn num_compressable(a: &str) -> bool {
    match a.parse::<u16>() {
        Err(_) => false,
        Ok(i) => a == i.to_string().as_str(),
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
pub enum Housenumber {
    CleanInt(u16),
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
        if !self.housenumbers.contains(&hn) {
            self.housenumbers.push(hn);
        }
    }
    pub fn sort_with(&mut self, hn_sort: impl FnMut(&Housenumber, &Housenumber) -> Ordering) {
        self.housenumbers.sort_by(hn_sort)
    }
    fn housenumber_iter<'a>(&'a self, w: &'a World) -> impl Iterator<Item = String> + 'a {
        self.housenumbers.iter().map(|s| match s {
            Housenumber::Index(i) => w.housenumbers[*i as usize].to_string(),
            Housenumber::CleanInt(i) => i.to_string(),
        })
    }
    pub fn iter_housenumbers_prefixed<'a>(
        &'a self,
        prefix: String,
        world: &'a World,
    ) -> impl Iterator<Item = String> + 'a {
        self.housenumber_iter(world)
            .filter(move |hn| hn.to_lowercase().starts_with(&prefix.to_lowercase()))
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
    pub fn iter_streets_prefixed<'a>(
        &'a self,
        prefix: String,
        world: &'a World,
    ) -> impl Iterator<Item = &'a String> {
        self.streets
            .iter()
            .map(|s| &world.unique_streets[s.index as usize])
            .filter(move |street| street.to_lowercase().starts_with(&prefix.to_lowercase()))
    }
    pub fn get_street<'a>(&'a self, street: &str, world: &'a World) -> Option<&Street> {
        self.streets.iter().find(|s| {
            &world.unique_streets[s.index as usize].to_lowercase() == &street.to_lowercase()
        })
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
    pub fn iter_zips_prefixed(&self, prefix: String) -> impl Iterator<Item = &String> {
        self.areas
            .iter()
            .filter(move |c| c.code.to_lowercase().starts_with(&prefix.to_lowercase()))
            .map(|c| &c.code)
    }
    pub fn get_postal_area(&self, zip: &str) -> Option<&PostalArea> {
        self.areas
            .iter()
            .find(|c| c.code.to_lowercase() == zip.to_lowercase())
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
    pub fn iter_cities_prefixed(&self, prefix: String) -> impl Iterator<Item = &String> {
        self.cities
            .iter()
            .filter(move |c| c.name.to_lowercase().starts_with(&prefix.to_lowercase()))
            .map(|c| &c.name)
    }
    pub fn get_city(&self, city_name: &str) -> Option<&City> {
        self.cities
            .iter()
            .find(|c| c.name.to_lowercase() == city_name.to_lowercase())
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
            true => Housenumber::CleanInt(housenumber.parse().unwrap()),
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
        } else {
            let mut country = Country::new(country_code);
            country.insert_address(city_name, zip, street_index, housenumber);
            self.countries.push(country);
        }
    }
    pub fn sort(&mut self) {
        self.countries.sort_by(|a, b| a.code.cmp(&b.code));
        for country in self.countries.iter_mut() {
            country.sort_with(|hn_a: &Housenumber, hn_b: &Housenumber| {
                let a = match hn_a {
                    Housenumber::CleanInt(v) => v.to_string(),
                    Housenumber::Index(i) => self
                        .housenumbers
                        .get(*i as usize)
                        .expect("Housenumber index greater then housenumber list length")
                        .to_string(),
                };
                let b = match hn_b {
                    Housenumber::CleanInt(v) => v.to_string(),
                    Housenumber::Index(i) => self
                        .housenumbers
                        .get(*i as usize)
                        .expect("Housenumber index greater then housenumber list length")
                        .to_string(),
                };
                a.cmp(&b)
            })
        }
    }
    // pub fn iter_country_codes_prefixed(&self, prefix: &str) -> impl Iterator<Item = &String> {
    //     let prefix = prefix.to_string();
    //     self.countries
    //         .iter()
    //         .map(|e| &e.code)
    //         .filter(move |e| e.starts_with(&prefix))
    // }
    pub fn count(&self) -> usize {
        self.countries.len()
    }
    pub fn get_country(&self, country_code: String) -> Option<&Country> {
        self.countries
            .iter()
            .find(|c| c.code.to_lowercase() == country_code.to_lowercase())
    }
}

fn compress(streets: SortedVec<String>, hn: SortedVec<String>, mut addresses: Vec<Address>) {
    let mut world = World::new(streets, hn);
    let len = addresses.len();
    let mut i = 0;
    while let Some(addr) = addresses.pop() {
        if i % 10000 == 0 {
            info!("Insert address {}/{} into world data structure", i, len);
            addresses.shrink_to_fit();
        }
        i += 1;
        world.insert_address(
            addr.country,
            addr.city,
            addr.postcode,
            addr.street,
            addr.housenumber,
        );
    }
    info!("Sorting every wolrd entry...");
    world.sort();
    info!(
        "Done. Dumping world containing {} countries to stdout...",
        world.count()
    );
    let stdout = io::stdout().lock();
    bincode::serialize_into(stdout, &world).unwrap();
    info!("Done!");
}

pub fn read_and_compress() -> Result<(), String> {
    info!("Reading jsonl from stdin...");
    let stdin = io::stdin().lock();
    let mut addresses: Vec<Address> = Vec::new();
    let mut streets: HashSet<String> = HashSet::new();
    let mut uncompressable_house_numbers: HashSet<String> = HashSet::new();
    for (i, item) in iter_items(stdin).enumerate() {
        if i % 100000 == 0 {
            info!(
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

    info!("Sort streets ({})...", streets.len());
    let streets_sorted: SortedVec<String> = streets.into_iter().collect::<Vec<_>>().into();
    info!(
        "Sort house numbers ({})...",
        uncompressable_house_numbers.len()
    );
    let housenumbers_sorted: SortedVec<String> = uncompressable_house_numbers
        .into_iter()
        .collect::<Vec<String>>()
        .into();

    info!(
        "Processed {} addresses, {} unique street names, {} unique uncompressable house numbers",
        addresses.len(),
        streets_sorted.len(),
        housenumbers_sorted.len()
    );

    compress(streets_sorted, housenumbers_sorted, addresses);

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn auto_correct_cc() {
        assert_eq!(
            autocorrect_country_code("India".to_string()),
            "IN".to_string()
        );
        // assert_eq!(autocorrect_country_code("Deutschland".to_string()), "DE".to_string());
        assert_eq!(
            autocorrect_country_code("Germany".to_string()),
            "DE".to_string()
        );
        assert_eq!(autocorrect_country_code("GB".to_string()), "GB".to_string());
        assert_eq!(autocorrect_country_code("CA".to_string()), "CA".to_string());
    }
}
