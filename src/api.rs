use std::sync::Arc;

use axum::{
    async_trait,
    extract::{FromRequest, Query, State},
    http::{self, Request, StatusCode},
    routing::get,
    Json, Router, 
};
use serde::Deserialize;

use crate::{compress::World, MAX_ITEMS_HEADER};

struct MaxItems(usize);

#[async_trait]
impl<S, B> FromRequest<S, B> for MaxItems
where
    // these bounds are required by `async_trait`
    B: Send + 'static,
    S: Send + Sync,
{
    type Rejection = http::StatusCode;

    async fn from_request(req: Request<B>, _state: &S) -> Result<Self, Self::Rejection> {
        match req.headers().get(MAX_ITEMS_HEADER).map(|v| v.to_str()) {
            Some(Ok(v)) => Ok(MaxItems(v.parse().unwrap_or(usize::MAX))),
            _ => Ok(MaxItems(usize::MAX)),
        }
    }
}

#[derive(Deserialize)]
struct GetHousenumbersQuery {
    country_code: String,
    city_name: String,
    zip: String,
    street: String,
    prefix: Option<String>,
}
async fn get_housenumbers(
    w: State<Arc<World>>,
    Query(q): Query<GetHousenumbersQuery>,
    MaxItems(m): MaxItems,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    // let TypedHeader(max) = max_reasults.unwrap_or(TypedHeader(usize::MAX));
    match w
        .get_country(q.country_code)
        .and_then(|country| country.get_city(q.city_name.as_str()))
        .and_then(|city| city.get_postal_area(q.zip.as_str()))
        .and_then(|postal_area| postal_area.get_street(q.street.as_str(), w.as_ref()))
    {
        None => Err((
            StatusCode::NOT_FOUND,
            "Country/city/zip/street not found".to_string(),
        )),
        Some(street) => Ok(Json(
            street
                .iter_housenumbers_prefixed(q.prefix.unwrap_or("".into()), w.as_ref())
                .take(m)
                .collect(),
        )),
    }
}

#[derive(Deserialize)]
struct GetStreetsQuery {
    country_code: String,
    city_name: String,
    zip: String,
    prefix: Option<String>,
}
async fn get_streets(
    w: State<Arc<World>>,
    Query(q): Query<GetStreetsQuery>,
    MaxItems(m): MaxItems,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    match w
        .get_country(q.country_code)
        .and_then(|country| country.get_city(q.city_name.as_str()))
        .and_then(|city| city.get_postal_area(q.zip.as_str()))
    {
        None => Err((StatusCode::NOT_FOUND, "Country/city/zip not found".to_string())),
        Some(city) => Ok(Json(
            city.iter_streets_prefixed(q.prefix.unwrap_or(String::new()), w.as_ref())
                .take(m)
                .cloned()
                .collect(),
        )),
    }
}

#[derive(Deserialize)]
struct GetZipsQuery {
    country_code: String,
    city_name: String,
    prefix: Option<String>,
}
async fn get_zips(
    w: State<Arc<World>>,
    Query(q): Query<GetZipsQuery>,
    MaxItems(m): MaxItems,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    match w
        .get_country(q.country_code)
        .and_then(|c| c.get_city(q.city_name.as_str()))
    {
        None => Err((StatusCode::NOT_FOUND, "Country/city not found".to_string())),
        Some(city) => Ok(Json(
            city.iter_zips_prefixed(q.prefix.unwrap_or(String::new()))
                .take(m)
                .cloned()
                .collect(),
        )),
    }
}

#[derive(Deserialize)]
struct GetCitiesQuery {
    country_code: String,
    prefix: Option<String>,
}
async fn get_cities(
    w: State<Arc<World>>,
    Query(q): Query<GetCitiesQuery>,
    MaxItems(m): MaxItems,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    match w.get_country(q.country_code.clone()) {
        None => Err((
            StatusCode::NOT_FOUND,
            format!("Country {} not found", q.country_code),
        )),
        Some(country) => Ok(Json(
            country
                .iter_cities_prefixed(q.prefix.unwrap_or(String::new()))
                .take(m)
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
