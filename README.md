# Maps address completion service

Serve auto completions for addresses,
like city names, zip codes, street names and house numbers.
Useful for e.g. webforms where a valid address has to be manually entered.

## Feature
- Works on OpenStreetMaps data
- Single digit millisecond response for every request
    - Tested with my 2014 potato notebook
    - Might be better in the future, doing iteration where binary search is possible
- Everything in RAM
- Serve address of the entire globe* with 350MB memory
- 1s - 3s startup time

\*OSM data is sparse on many parts, europe and north america best mapped

## TLDR
```
cargo install maps-address-completion-service
curl -s https://download.geofabrik.de/europe/greece-latest.osm.pbf |
    macs parse |
    macs compress > greece.world
macs serve --world greece.world
```

## Workflow of this application

This applications is split into three stages,
which allow to see what's going on and modify data
inbetween.

For the example commands, it is assumed, that
you installed the package (e.g. `cargo install --path .`).
The first two steps are stdin, stdout based.

### 1. Parse data from OpenStreetMaps
The first step converts from OpenStreetMaps data in the
`*.osm.pbf` format into json lines of the format:
```json
{"country":"ZA","city":"Pinelands","postcode":"7405","street":"La Provence","housenumber":"1"}
{"country":"ZA","city":"Pinelands","postcode":"7405","street":"Ringwood Drive","housenumber":"2"}
...
```

You can download maps from the [Geofabrik](https://download.geofabrik.de/-) site.  
There are entire continents as well as just regions.

```bash
wget https://download.geofabrik.de/europe/great-britain-latest.osm.pbf -O great-britain.osm.pbf
cat great-britain.osm.pbf | macs parse > map.jsonl
# Or compress it directly
cat great-britain.osm.pbf | macs parse | xz > maps.jsonl.xz
```

### 2. Compress into custom data structure
Here, everything get's sorted, street names and house numbers deduplicated, etc.
The resulting object it pretty much a memory representation of the final structure
and will therefore be a good index for how much memory will be consumed.

```bash
cat maps.jsonl | macs compress > great-britain.world
ls -lah great-britain.world
```

### 3. Server via HTTP

The server can be startet with

```bash
macs serve -w great-britain.world
macs serve --world great-britain.world --port 3000 --ip 127.0.0.1
```
```
[2023-10-16T22:45:11Z INFO macs::serve] Loading from world file "gb.world"...
[2023-10-16T22:45:11Z INFO macs::serve] World loadded, containing 3 countries.
[2023-10-16T22:45:11Z INFO macs::serve] Serve on 127.0.0.1:3000...
```
Now we can query:
```
curl "http://127.0.0.1:3000/cities?country_code=GB&prefix=Lon&max=10"
curl "http://127.0.0.1:3000/zips?country_code=GB&city_name=London&prefix=E1"
curl "http://127.0.0.1:3000/streets?country_code=GB&city_name=London&zip=W4 1LB&prefix="
curl "http://127.0.0.1:3000/housenumbers?country_code=GB&city_name=London&zip=E17&prefix="
```

## Notes
- This is a service intended to be used by backends rather then frontends. If used by frontends, configure
  reverse proxy accordingly. When reverse proxying, inject a low `max` query and enable rate limiting.
  The small request - big response nature might be attractive for DOSing.
- OSM has a lot of faulty data, like cities named `"<format"`, quoted house numbers or similiar things.
- There is still room for performance improvements, but it's doing fine already.


## Some fun numbers
These are some results or just read out of the logs.  
These all relate to OSM data, which for example maps barely
the most important cities in Africa. It also
injects faulty data.

- The europe world struct is 169MiB
- The entire world struct is 203MiB
- The entire world has ca. 26 Mio addresses
- The entire world has ca. 736.000 unique street names
- The entire world has ca. 378.000 unique house numbers which are not just integers (e.g. 1A)

## Contribute
Contributions are very welcomed. If you wish any new features, feel free to open
an issue.
When opening a pull request, please use `cargo fmt` and keep the code as simple as possible.

### Potential futures improvements
- prefix optional (easy)
- search countries, by code and by name
- skip one layer
    - given country, allow to list zip codes (no city)
    - given city, allow to list streets
- When 404, specify what exactly has not been found
- Do binary search whenever possible (improve SortedVec struct?)
