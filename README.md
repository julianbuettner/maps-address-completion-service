# Maps address completion service

Serve auto completions for addresses,
like city names, zip codes, street names and house numbers.
Useful for e.g. webforms where a valid address has to be entered manually.

## Feature
- Works on OpenStreetMaps data
- ~1ms response time, probably less
    - Tested with my 2014 potato notebook, Firefox
- Everything in RAM
    - On average 8 bytes per address - tuple (country, city, zip, street, house)
- Serve address of the entire globe* with 302MiB memory
- 1s - 2s startup time to load all OSM addresses in existence

\* OSM data is missing many cities, so it's very much incomplete.

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

For the short guide, it is assumed, that
you installed the package (e.g. `cargo install --path .` / 
`cargo install maps-address-completion-service`).
The first two steps are stdin, stdout based.

### 1. Parse data from OpenStreetMaps
The first step converts from OpenStreetMaps data
(`*.osm.pbf`) to json lines of the following format:
```json
{"country":"ZA","city":"Pinelands","postcode":"7405","street":"La Provence","housenumber":"1"}
{"country":"ZA","city":"Pinelands","postcode":"7405","street":"Ringwood Drive","housenumber":"2"}
```

You can download OSM maps from the [Geofabrik](https://download.geofabrik.de/-) site.  
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
The building process requires between 3GiB and 6GiB of memory for the entire globe.

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
# First get cities
curl http://localhost:3000/cities --url-query "country_code=GB"

# Then ZIP codes
curl http://localhost:3000/zips\
    --url-query "country_code=GB" \
    --url-query "city_name=London"

# Then streets
curl http://localhost:3000/streets \
    --url-query "country_code=GB" \
    --url-query "city_name=London" \
    --url-query "zip=WC2R 0JR"

# Then house numbers
curl http://localhost:3000/housenumbers \
    --url-query "country_code=GB" \
    --url-query "city_name=London" \
    --url-query "zip=WC2R 0JR" \
    --url-query "street=Strand"

# All requests support prefix searching
curl http://localhost:3000/cities --url-query "country_code=GB" --url-query "prefix=Lon"

# All requests support result limiting
curl http://localhost:3000/cities --url-query "country_code=GB" -H "max-items: 16"
```

## Notes and Details
- The server does not log requests.
- All results are a one dimensional list of strings, json.
- The country code is defined in the ISO-3166 standart.
- Generally all data is in sorted vectors, not in hashmaps. This
  compactness results in an optimal memory usage and allows for binary searching.
  Therefore the timecomplexity of a request is in O(log n), not O(1).
- This is a service intended to be used by backends rather than frontends. If used by frontends, configure
  reverse proxy accordingly. When reverse proxying, inject a low `max-items: 123` header and enable rate limiting.
  The small request - big response nature might be attractive for DOSing.
- OSM has a lot of faulty data, like cities named `"<format"` or `1,2,3`, quoted house numbers or similiar things.
- There is still room for performance improvements, but it's doing pretty fine already.


## Some fun numbers
These are some numbers I stumbled across while making this little project.
These all relate to OSM data, which for example maps barely the most important cities in Africa. It also
injects faulty data. So please take those numbers with a big grain of salt.

- The europe world struct is 169MiB
- The entire world struct is 203MiB
- The entire world has ca. 26 Mio addresses*
- The entire world has ca. 736.000 unique street names*
- The entire world has ca. 378.000 unique house numbers which are not just integers (e.g. 1A)*

\* Again, based on OSM Data, which is faulty and incomplete

## Contribute
Contributions are very welcomed. If you wish any new features, feel free to open an issue.
When opening a pull request, please use `cargo fmt` and keep the code as simple as possible.

### Potential futures improvements
- search countries, by code and by name
- skip one layer
    - given country, allow to list zip codes (no city)
    - given city, allow to list streets
- When 404, specify what exactly has not been found
- Do binary search whenever possible (improve SortedVec to contain search closure?)
