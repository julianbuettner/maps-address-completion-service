use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{compress::World, MAX_ITEMS};

#[derive(Deserialize)]
struct GetHousenumbersQuery {
    max: Option<usize>,
    country_code: String,
    city_name: String,
    zip: String,
    street: String,
    prefix: String,
}
async fn get_housenumbers(
    w: State<Arc<World>>,
    Query(q): Query<GetHousenumbersQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    match w
        .get_country(q.country_code)
        .map(|country| country.get_city(q.city_name.as_str()))
        .flatten()
        .map(|city| city.get_postal_area(q.zip.as_str()))
        .flatten()
        .map(|postal_area| postal_area.get_street(q.street.as_str(), w.as_ref()))
        .flatten()
    {
        None => Err((
            StatusCode::NOT_FOUND,
            format!("Country/city/zip/street not found"),
        )),
        Some(street) => Ok(Json(
            street
                .iter_housenumbers_prefixed(q.prefix, w.as_ref())
                .take(q.max.unwrap_or(usize::MAX))
                .collect(),
        )),
    }
}

#[derive(Deserialize)]
struct GetStreetsQuery {
    max: Option<usize>,
    country_code: String,
    city_name: String,
    zip: String,
    prefix: String,
}
async fn get_streets(
    w: State<Arc<World>>,
    Query(q): Query<GetStreetsQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    match w
        .get_country(q.country_code)
        .map(|country| country.get_city(q.city_name.as_str()))
        .flatten()
        .map(|city| city.get_postal_area(q.zip.as_str()))
        .flatten()
    {
        None => Err((StatusCode::NOT_FOUND, format!("Country/city/zip not found"))),
        Some(city) => Ok(Json(
            city.iter_streets_prefixed(q.prefix, w.as_ref())
                .take(q.max.unwrap_or(usize::MAX))
                .cloned()
                .collect(),
        )),
    }
}

#[derive(Deserialize)]
struct GetZipsQuery {
    max: Option<usize>,
    country_code: String,
    city_name: String,
    prefix: String,
}
async fn get_zips(
    w: State<Arc<World>>,
    Query(q): Query<GetZipsQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    match w
        .get_country(q.country_code)
        .map(|c| c.get_city(q.city_name.as_str()))
        .flatten()
    {
        None => Err((StatusCode::NOT_FOUND, format!("Country/city not found"))),
        Some(city) => Ok(Json(
            city.iter_zips_prefixed(q.prefix)
                .take(q.max.unwrap_or(usize::MAX))
                .cloned()
                .collect(),
        )),
    }
}

#[derive(Deserialize)]
struct GetCitiesQuery {
    max: Option<usize>,
    country_code: String,
    prefix: String,
}
async fn get_cities(
    w: State<Arc<World>>,
    Query(q): Query<GetCitiesQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    match w.get_country(q.country_code.clone()) {
        None => Err((
            StatusCode::NOT_FOUND,
            format!("Country {} not found", q.country_code),
        )),
        Some(country) => Ok(Json(
            country
                .iter_cities_prefixed(q.prefix)
                .take(q.max.unwrap_or(usize::MAX))
                .take(MAX_ITEMS)
                .cloned()
                .collect(),
        )),
    }
}

pub fn get_app(world: World) -> Router {
    Router::new()
        .route("/cities", get(get_cities))
        .route("/zips", get(get_zips))
        .route("/streets", get(get_streets))
        .route("/housenumbers", get(get_housenumbers))
        .with_state(Arc::new(world))
}
