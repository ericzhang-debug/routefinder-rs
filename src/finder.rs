use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use crate::database::Database;
use crate::graph::{Graph, distance_nm, bearing_deg, bearing_diff};
use crate::models::*;

const ROUTE_SID: &str = "SID";
const ROUTE_STAR: &str = "STAR";
const ROUTE_DCT: &str = "DCT";
const MAX_BEARING_DEV: f64 = 90.0;

pub struct RouteFinder {
    graph: Mutex<Graph>,
    wpt_id_to_vid: HashMap<i32, usize>,
    airport_id_to_vid: HashMap<i32, usize>,
    pub db: Mutex<Database>,
}

impl RouteFinder {
    pub fn new(db: Database) -> rusqlite::Result<Self> {
        let mut graph = Graph::new();
        let mut wpt_id_to_vid = HashMap::new();
        let mut airport_id_to_vid = HashMap::new();

        for (id, icao, lat, lon) in db.fetch_all_airports()? {
            let vid = graph.add_vertex(&icao, lat, lon);
            airport_id_to_vid.insert(id, vid);
        }
        for (id, ident, lat, lon) in db.fetch_all_waypoints()? {
            let vid = graph.add_vertex(&ident, lat, lon);
            wpt_id_to_vid.insert(id, vid);
        }
        for (airway, w1, w2) in db.fetch_all_airway_legs()? {
            if let (Some(&v1), Some(&v2)) = (wpt_id_to_vid.get(&w1), wpt_id_to_vid.get(&w2)) {
                let rid = graph.get_or_add_route(&airway);
                graph.add_undirected_edge(v1, v2, rid);
            }
        }
        graph.get_or_add_route(ROUTE_DCT);
        graph.get_or_add_route(ROUTE_SID);
        graph.get_or_add_route(ROUTE_STAR);

        Ok(Self { graph: Mutex::new(graph), wpt_id_to_vid, airport_id_to_vid, db: Mutex::new(db) })
    }

    pub fn find_route(&self, dep: &str, arr: &str) -> Result<RouteResult, String> {
        let dep = dep.to_uppercase();
        let arr = arr.to_uppercase();

        let mut graph = self.graph.lock().map_err(|e| e.to_string())?;
        let db = self.db.lock().map_err(|e| e.to_string())?;

        let dep_vids = graph.find_vertex_ids(&dep);
        let arr_vids = graph.find_vertex_ids(&arr);
        let dep_vid = *dep_vids.first().ok_or_else(|| format!("Departure '{}' not found", dep))?;
        let arr_vid = *arr_vids.first().ok_or_else(|| format!("Arrival '{}' not found", arr))?;

        // SID/STAR
        let sid_rid = graph.route_ids.get(ROUTE_SID).copied();
        let star_rid = graph.route_ids.get(ROUTE_STAR).copied();
        let mut seen = HashSet::new();

        for (icao, is_dep, target_vid) in [(&dep, true, arr_vid), (&arr, false, dep_vid)] {
            if seen.contains(icao) { continue; }
            seen.insert(icao.clone());

            if let Some(av) = graph.find_first_id(icao) {
                let (alat, alon) = (graph.vertices[av].latitude, graph.vertices[av].longitude);
                let target_bearing: Option<f64> = {
                    let tv = &graph.vertices[target_vid];
                    Some(if is_dep { bearing_deg(alat, alon, tv.latitude, tv.longitude) }
                         else { bearing_deg(tv.latitude, tv.longitude, alat, alon) })
                };

                if let Ok(Some(aid)) = db.find_airport_id(icao) {
                    if let Ok((sid_links, star_links)) = db.fetch_terminal_fixes(aid) {
                        if let Some(rid) = sid_rid {
                            for (_, w) in &sid_links {
                                if let Some(&wv) = self.wpt_id_to_vid.get(w) {
                                    let wt = &graph.vertices[wv];
                                    if distance_nm(alat, alon, wt.latitude, wt.longitude) < 400.0 {
                                        if let (true, Some(ref b)) = (is_dep, target_bearing) {
                                            if bearing_diff(bearing_deg(alat, alon, wt.latitude, wt.longitude), *b) > MAX_BEARING_DEV { continue; }
                                        }
                                        graph.add_directed_edge(av, wv, rid);
                                    }
                                }
                            }
                        }
                        if let Some(rid) = star_rid {
                            for (w, _) in &star_links {
                                if let Some(&wv) = self.wpt_id_to_vid.get(w) {
                                    let wt = &graph.vertices[wv];
                                    if distance_nm(alat, alon, wt.latitude, wt.longitude) < 400.0 {
                                        if let (false, Some(ref b)) = (is_dep, target_bearing) {
                                            if bearing_diff(bearing_deg(wt.latitude, wt.longitude, alat, alon), *b) > MAX_BEARING_DEV { continue; }
                                        }
                                        graph.add_directed_edge(wv, av, rid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // A*
        let (_, path) = graph.a_star(dep_vid, arr_vid);
        let mut result = RouteResult::default();

        if path.is_empty() {
            let dd = distance_nm(graph.vertices[dep_vid].latitude, graph.vertices[dep_vid].longitude, graph.vertices[arr_vid].latitude, graph.vertices[arr_vid].longitude);
            result.route = format!("{} DCT {}", dep, arr);
            result.legs.push(Leg { from_name: dep.clone(), to_name: arr.clone(), route: "DCT".into(), distance_nm: dd });
            result.waypoints.push(Waypoint { ident: dep.clone(), name: None, latitude: graph.vertices[dep_vid].latitude, longitude: graph.vertices[dep_vid].longitude });
            result.waypoints.push(Waypoint { ident: arr.clone(), name: None, latitude: graph.vertices[arr_vid].latitude, longitude: graph.vertices[arr_vid].longitude });
        } else {
            for (from, to, route, dist) in &graph.extract_legs(&path, dep_vid, arr_vid) {
                result.legs.push(Leg { from_name: from.clone(), to_name: to.clone(), route: route.clone(), distance_nm: *dist });
            }
            for &vid in &path {
                let v = &graph.vertices[vid];
                result.waypoints.push(Waypoint { ident: v.name.clone(), name: None, latitude: v.latitude, longitude: v.longitude });
            }
            result.route = generate_route_string(&result.legs);
        }

        result.total_distance_nm = result.legs.iter().map(|l| l.distance_nm).sum();
        result.departure = db.fetch_airport_info(&dep).ok().flatten();
        result.arrival = db.fetch_airport_info(&arr).ok().flatten();
        result.nav_config = Some(db.fetch_config().unwrap_or_default());

        let dep_name = result.departure.as_ref().map(|a| a.name.as_str()).unwrap_or("");
        let arr_name = result.arrival.as_ref().map(|a| a.name.as_str()).unwrap_or("");

        let wpt_len = result.waypoints.len();
        for (i, wp) in result.waypoints.iter_mut().enumerate() {
            if i == 0 && wp.ident == dep { wp.name = Some(dep_name.into()); }
            else if i == wpt_len - 1 && wp.ident == arr { wp.name = Some(arr_name.into()); }
            else if let Ok(Some(dn)) = db.fetch_waypoint_display_name(&wp.ident) { wp.name = Some(dn); }
        }

        if path.len() >= 2 {
            let fw = &graph.vertices[path[1]].name;
            result.suggested_sids = db.find_procedures_by_wpt(fw, &dep, "1").unwrap_or_default();
        }
        if path.len() >= 2 {
            let lw = &graph.vertices[path[path.len()-2]].name;
            result.suggested_stars = db.find_procedures_by_wpt(lw, &arr, "2").unwrap_or_default();
        }
        result.all_sids = db.fetch_all_procedures(&dep, "1").unwrap_or_default();
        result.all_stars = db.fetch_all_procedures(&arr, "2").unwrap_or_default();

        Ok(result)
    }

    pub fn get_airport_info(&self, icao: &str) -> Result<Option<AirportInfo>, String> {
        self.db.lock().map_err(|e| e.to_string())?.fetch_airport_info(icao).map_err(|e| e.to_string())
    }

    pub fn get_procedures(&self, icao: &str, ptype: &str) -> Result<Vec<ProcedureInfo>, String> {
        self.db.lock().map_err(|e| e.to_string())?.fetch_all_procedures(icao, ptype).map_err(|e| e.to_string())
    }

    pub fn get_suggested_procedures(&self, icao: &str, wpt: &str, ptype: &str) -> Result<Vec<ProcedureInfo>, String> {
        self.db.lock().map_err(|e| e.to_string())?.find_procedures_by_wpt(wpt, icao, ptype).map_err(|e| e.to_string())
    }

    pub fn get_nav_config(&self) -> Result<NavConfig, String> {
        self.db.lock().map_err(|e| e.to_string())?.fetch_config().map_err(|e| e.to_string())
    }

    pub fn get_waypoint_info(&self, ident: &str) -> Result<Vec<WaypointInfo>, String> {
        self.db.lock().map_err(|e| e.to_string())?.fetch_waypoint_info(ident).map_err(|e| e.to_string())
    }

    pub fn get_airway_waypoints(&self, airway: &str) -> Result<Option<AirwayInfo>, String> {
        self.db.lock().map_err(|e| e.to_string())?.fetch_airway_waypoints(airway).map_err(|e| e.to_string())
    }
}

fn generate_route_string(legs: &[Leg]) -> String {
    if legs.is_empty() { return String::new(); }
    if legs.len() == 1 { return format!("{} {} {}", legs[0].from_name, legs[0].route, legs[0].to_name); }
    let mut parts = vec![legs[0].from_name.clone(), legs[0].route.clone()];
    let mut last = "";
    for i in 1..legs.len() {
        if legs[i].route == last { continue; }
        last = &legs[i].route;
        parts.push(legs[i-1].to_name.clone());
        parts.push(legs[i].route.clone());
    }
    parts.push(legs.last().unwrap().to_name.clone());
    parts.join(" ")
}
