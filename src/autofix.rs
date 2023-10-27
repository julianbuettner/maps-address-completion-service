use crate::{
    compress::{City, Country, World},
    parse::{Address, IncompleteAddress},
};

pub fn is_unfixable(a: &IncompleteAddress) -> bool {
    // complete -> fixable
    // country missing -> fixable (city, zip)
    // city missing -> fixable (zip)
    // zip missing -> fixable (country, city, street)
    // street or housenumber missing -> unfixable

    if a.is_complete() {
        return false;
    }
    if a.housenumber.is_none() || a.street.is_none() {
        return true;
    }
    // Need two of three
    if [&a.country, &a.city, &a.postcode]
        .iter()
        .filter(|o| o.is_some())
        .count()
        >= 2
    {
        return false;
    }
    true
}

fn get_country_from_city_zip(w: &World, city: String, zip: Option<String>) -> Option<&Country> {
    let mut potential_countries: Vec<(&Country, &City)> = Vec::new();
    for potential_country in w.iter_countries() {
        for potential_city in potential_country.iter_cities().filter(|c| c.name == city) {
            potential_countries.push((potential_country, potential_city));
        }
    }
    if let Some(zip) = &zip {
        potential_countries = potential_countries
            .into_iter()
            .filter(|(_country, city)| city.get_postal_area(zip.as_str()).is_some())
            .collect();
    }
    match potential_countries.len() {
        0 => None,
        1 => Some(potential_countries[0].0),
        n => {
            log::debug!(
                "There were {} matching countries for city/zip {:?}: {:?}",
                n,
                (city, zip),
                potential_countries
                    .into_iter()
                    .map(|(country, city)| format!("[{} - {}]", country.code, city.name))
                    .collect::<Vec<String>>()
            );
            None
        }
    }
}

fn get_city_from_country_zip(w: &World, country: Option<String>, zip: String) -> Option<&City> {
    let mut potential_cities: Vec<(&Country, &City)> = Vec::new();
    for portential_country in w.iter_countries() {
        if let Some(c_code) = &country {
            if c_code != &portential_country.code {
                continue;
            }
        }
        for potential_city in portential_country.iter_cities() {
            if let Some(_zip) = potential_city.get_postal_area(zip.as_str()) {
                potential_cities.push((portential_country, potential_city))
            }
        }
    }
    match potential_cities.len() {
        0 => None,
        1 => Some(potential_cities[0].1),
        n => {
            log::debug!(
                "There were {} matching cities for country/zip {:?}: {:?}",
                n,
                (country, zip),
                potential_cities
                    .into_iter()
                    .map(|(country, city)| format!("[{} - {}]", country.code, city.name))
                    .collect::<Vec<String>>()
            );
            None
        }
    }
}

// fn get_zip_from_country_city_street(w: &World, country:)

pub fn try_autofixing(
    w: &World,
    mut incomplete_addresses: Vec<IncompleteAddress>,
) -> (Vec<Address>, Vec<IncompleteAddress>) {
    let mut unfixable: Vec<IncompleteAddress> = Vec::new();
    let mut fixed: Vec<Address> = Vec::new();
    while let Some(IncompleteAddress {
        country,
        city,
        postcode,
        street,
        housenumber,
    }) = incomplete_addresses.pop()
    {
        match (country, city, postcode, street, housenumber) {
            (Some(co), Some(ci), Some(po), Some(st), Some(hn)) => fixed.push(Address {
                country: co,
                city: ci,
                postcode: po,
                street: st,
                housenumber: hn,
            }),
            (None, Some(ci), po, st, hn) => {
                match get_country_from_city_zip(w, ci.clone(), po.clone()) {
                    None => unfixable.push(IncompleteAddress {
                        country: None,
                        city: Some(ci),
                        postcode: po,
                        street: st,
                        housenumber: hn,
                    }),
                    Some(country) => incomplete_addresses.push(IncompleteAddress {
                        country: Some(country.code.clone()),
                        city: Some(ci),
                        postcode: po,
                        street: st,
                        housenumber: hn,
                    }),
                }
            }
            (co, None, Some(zip), st, hn) => match get_city_from_country_zip(w, co.clone(), zip.clone()) {
                None => unfixable.push(IncompleteAddress {
                    country: co,
                    city: None,
                    postcode: Some(zip),
                    street: st,
                    housenumber: hn,
                }),
                Some(city) => incomplete_addresses.push(IncompleteAddress {
                    country: co,
                    city: Some(city.name.clone()),
                    postcode: Some(zip),
                    street: st,
                    housenumber: hn,
                }),
            },
            _ => (),
        }
    }
    (fixed, unfixable)
}
