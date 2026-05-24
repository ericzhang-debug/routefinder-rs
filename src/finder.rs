use std::collections::{HashMap, HashSet};
use std::path::Path;
use crate::database::NavDatabase;
use crate::graph::{Graph, distance_nm, bearing_deg, bearing_diff};
use crate::models::*;

const ROUTE_SID: &str = "SID";
const ROUTE_STAR: &str = "STAR";
const ROUTE_DCT: &str = "DCT";
const MAX_BEARING_DEV: f64 = 90.0;

pub struct RouteFinder {
    db: NavDatabase,
    graph: Graph,
    airport_id_to_vid: HashMap<i32, usize>,
    wpt_id_to_vid: HashMap<i32, usize>,
    initialized: bool,
    initialized_airports: HashSet<String>,
}

impl RouteFinder {
    pub fn new(db_path: &Path) -> rusqlite::Result<Self> {
        let db = NavDatabase::open(db_path)?;
        Ok(Self {
            db,
            graph: Graph::new(),
            airport_id_to_vid: HashMap::new(),
            wpt_id_to_vid: HashMap::new(),
            initialized: false,
            initialized_airports: HashSet::new(),
        })
    }

    pub fn initialize(&mut self) -> rusqlite::Result<()> {
        if self.initialized { return Ok(()); }

        self.db.create_indices()?;

        for (id, icao, lat, lon) in self.db.fetch_all_airports()? {
            let vid = self.graph.add_vertex(&icao, lat, lon);
            self.airport_id_to_vid.insert(id, vid);
        }
        for (id, ident, lat, lon) in self.db.fetch_all_waypoints()? {
            let vid = self.graph.add_vertex(&ident, lat, lon);
            self.wpt_id_to_vid.insert(id, vid);
        }
        for (airway, wpt1, wpt2) in self.db.fetch_all_airway_legs()? {
            if let (Some(&v1), Some(&v2)) = (
                self.wpt_id_to_vid.get(&wpt1),
                self.wpt_id_to_vid.get(&wpt2),
            ) {
                let rid = self.graph.get_or_add_route(&airway);
                self.graph.add_undirected_edge(v1, v2, rid);
            }
        }
        self.graph.get_or_add_route(ROUTE_DCT);
        self.graph.get_or_add_route(ROUTE_SID);
        self.graph.get_or_add_route(ROUTE_STAR);

        self.initialized = true;
        eprintln!(
            "[RouteFinder] Loaded {} vertices, {} route names",
            self.graph.vertices.len(), self.graph.route_names.len(),
        );
        Ok(())
    }

    fn init_airport(&mut self, icao: &str, target_vid: Option<usize>, is_dep: bool)
        -> rusqlite::Result<()>
    {
        let icao = icao.to_uppercase();
        if self.initialized_airports.contains(&icao) {
            return Ok(());
        }

        let sid_rid = self.graph.route_ids.get(ROUTE_SID).copied();
        let star_rid = self.graph.route_ids.get(ROUTE_STAR).copied();

        let airport_vid = match self.graph.find_first_id(&icao) {
            Some(v) => v,
            None => return Ok(()),
        };
        let airport_lat = self.graph.vertices[airport_vid].latitude;
        let airport_lon = self.graph.vertices[airport_vid].longitude;

        let target_bearing: Option<f64> = target_vid.map(|tid| {
            let tv = &self.graph.vertices[tid];
            if is_dep {
                bearing_deg(airport_lat, airport_lon, tv.latitude, tv.longitude)
            } else {
                bearing_deg(tv.latitude, tv.longitude, airport_lat, airport_lon)
            }
        });

        let airport_id = match self.db.find_airport_id_by_icao(&icao)? {
            Some(id) => id,
            None => return Ok(()),
        };

        let (sid_links, star_links) = self.db.fetch_terminal_fixes_for_airport(airport_id)?;

        if let Some(rid) = sid_rid {
            for (_, wpt_db_id) in &sid_links {
                if let Some(&wpt_vid) = self.wpt_id_to_vid.get(wpt_db_id) {
                    let wpt = &self.graph.vertices[wpt_vid];
                    let dist = distance_nm(airport_lat, airport_lon, wpt.latitude, wpt.longitude);
                    if dist < 400.0 {
                        if let Some(ref_b) = target_bearing {
                            if is_dep {
                                let brg = bearing_deg(airport_lat, airport_lon, wpt.latitude, wpt.longitude);
                                if bearing_diff(brg, ref_b) > MAX_BEARING_DEV { continue; }
                            }
                        }
                        self.graph.add_directed_edge(airport_vid, wpt_vid, rid);
                    }
                }
            }
        }

        if let Some(rid) = star_rid {
            for (wpt_db_id, _) in &star_links {
                if let Some(&wpt_vid) = self.wpt_id_to_vid.get(wpt_db_id) {
                    let wpt = &self.graph.vertices[wpt_vid];
                    let dist = distance_nm(airport_lat, airport_lon, wpt.latitude, wpt.longitude);
                    if dist < 400.0 {
                        if let Some(ref_b) = target_bearing {
                            if !is_dep {
                                let brg = bearing_deg(wpt.latitude, wpt.longitude, airport_lat, airport_lon);
                                if bearing_diff(brg, ref_b) > MAX_BEARING_DEV { continue; }
                            }
                        }
                        self.graph.add_directed_edge(wpt_vid, airport_vid, rid);
                    }
                }
            }
        }

        self.initialized_airports.insert(icao);
        Ok(())
    }

    pub fn find_route(&mut self, departure: &str, arrival: &str) -> rusqlite::Result<RouteResult> {
        if !self.initialized {
            self.initialize()?;
        }

        let departure = departure.to_uppercase();
        let arrival = arrival.to_uppercase();

        let dep_vids = self.graph.find_vertex_ids(&departure);
        let arr_vids = self.graph.find_vertex_ids(&arrival);
        let dep_vid = *dep_vids.first()
            .ok_or_else(|| rusqlite::Error::InvalidParameterName(format!("Departure '{}' not found", departure)))?;
        let arr_vid = *arr_vids.first()
            .ok_or_else(|| rusqlite::Error::InvalidParameterName(format!("Arrival '{}' not found", arrival)))?;

        self.init_airport(&departure, Some(arr_vid), true)?;
        self.init_airport(&arrival, Some(dep_vid), false)?;

        let dep_v = &self.graph.vertices[dep_vid].clone();
        let arr_v = &self.graph.vertices[arr_vid].clone();
        let direct_dist = distance_nm(dep_v.latitude, dep_v.longitude, arr_v.latitude, arr_v.longitude);
        let mut result = RouteResult::default();

        let (_best_dist, path, _expanded) = self.graph.a_star(dep_vid, arr_vid);

        if path.is_empty() {
            result.route = format!("{} DCT {}", departure, arrival);
            result.legs.push(Leg { from_name: departure.clone(), to_name: arrival.clone(), route: "DCT".into(), distance_nm: direct_dist });
            result.waypoints.push(Waypoint { name: departure.clone(), display_name: String::new(), latitude: dep_v.latitude, longitude: dep_v.longitude });
            result.waypoints.push(Waypoint { name: arrival.clone(), display_name: String::new(), latitude: arr_v.latitude, longitude: arr_v.longitude });
        } else {
            for (from, to, route, dist) in &self.graph.extract_legs(&path, dep_vid, arr_vid) {
                result.legs.push(Leg { from_name: from.clone(), to_name: to.clone(), route: route.clone(), distance_nm: *dist });
            }
            for &vid in &path {
                let v = &self.graph.vertices[vid];
                result.waypoints.push(Waypoint { name: v.name.clone(), display_name: String::new(), latitude: v.latitude, longitude: v.longitude });
            }
            result.route = generate_route_string(&result.legs);
        }

        result.departure_info = self.db.fetch_airport_detail(&departure)?;
        result.arrival_info = self.db.fetch_airport_detail(&arrival)?;
        result.nav_config = Some(self.db.fetch_config()?);

        for wp in &mut result.waypoints {
            if let Ok(Some(display)) = self.db.fetch_waypoint_display_name(&wp.name) {
                wp.display_name = display;
            }
        }

        if path.len() >= 2 {
            result.suggested_sids = self.db.find_procedures_by_wpt(
                &self.graph.vertices[path[1]].name, &departure, "1"
            )?;
        }
        if path.len() >= 2 {
            result.suggested_stars = self.db.find_procedures_by_wpt(
                &self.graph.vertices[path[path.len() - 2]].name, &arrival, "2"
            )?;
        }

        result.all_sids = self.db.fetch_all_procedures(&departure, "1")?;
        result.all_stars = self.db.fetch_all_procedures(&arrival, "2")?;

        Ok(result)
    }
}

fn generate_route_string(legs: &[Leg]) -> String {
    if legs.is_empty() {
        return String::new();
    }
    if legs.len() == 1 {
        return format!("{} {} {}", legs[0].from_name, legs[0].route, legs[0].to_name);
    }
    let mut parts = vec![legs[0].from_name.clone(), legs[0].route.clone()];
    let mut last_route = String::new();
    for i in 1..legs.len() {
        if legs[i].route == last_route {
            continue;
        }
        last_route = legs[i].route.clone();
        parts.push(legs[i - 1].to_name.clone());
        parts.push(legs[i].route.clone());
    }
    parts.push(legs.last().unwrap().to_name.clone());
    parts.join(" ")
}
