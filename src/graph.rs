use std::collections::{BinaryHeap, HashMap};
use std::f64::consts::PI;

const EARTH_RADIUS_NM: f64 = 6378.137 / 1.852;

pub fn distance_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rad = |x: f64| x * PI / 180.0;
    let a = rad(lat1) - rad(lat2);
    let b = rad(lon1) - rad(lon2);
    let s = 2.0 * ((a/2.0).sin().powi(2) + rad(lat1).cos() * rad(lat2).cos() * (b/2.0).sin().powi(2)).sqrt().asin();
    s * EARTH_RADIUS_NM
}

pub fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = |x: f64| x * PI / 180.0;
    let (rl1, rl2, dl) = (r(lat1), r(lat2), r(lon2 - lon1));
    let mut b = (dl.sin() * rl2.cos()).atan2(rl1.cos() * rl2.sin() - rl1.sin() * rl2.cos() * dl.cos()).to_degrees();
    if b < 0.0 { b += 360.0; }
    b
}

pub fn bearing_diff(a: f64, b: f64) -> f64 { let d = (a - b).abs(); d.min(360.0 - d) }

#[derive(Debug, Clone, Copy)]
pub struct Vec3 { pub x: f64, pub y: f64, pub z: f64 }

impl Vec3 {
    #[inline] pub fn chord_nm(&self, o: &Vec3) -> f64 {
        let (dx, dy, dz) = (self.x - o.x, self.y - o.y, self.z - o.z);
        (dx*dx + dy*dy + dz*dz).sqrt() * EARTH_RADIUS_NM
    }
}

pub fn lat_lon_to_vec3(lat: f64, lon: f64) -> Vec3 {
    let (rl, rln) = (lat * PI / 180.0, lon * PI / 180.0);
    Vec3 { x: rl.cos() * rln.cos(), y: rl.cos() * rln.sin(), z: rl.sin() }
}

#[derive(Debug, Clone)]
pub struct Vertex { pub id: usize, pub name: String, pub latitude: f64, pub longitude: f64, pub pos3: Vec3 }

#[derive(Debug, Clone)]
pub struct Edge { pub target: usize, pub distance: f64, pub route_id: usize }

pub struct Graph {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Vec<Edge>>,
    pub name_index: HashMap<String, Vec<usize>>,
    pub route_ids: HashMap<String, usize>,
    pub route_names: Vec<String>,
}

impl Graph {
    pub fn new() -> Self { Self { vertices: vec![], edges: vec![], name_index: HashMap::new(), route_ids: HashMap::new(), route_names: vec![] } }

    pub fn add_vertex(&mut self, name: &str, lat: f64, lon: f64) -> usize {
        let id = self.vertices.len();
        self.vertices.push(Vertex { id, name: name.to_string(), latitude: lat, longitude: lon, pos3: lat_lon_to_vec3(lat, lon) });
        self.edges.push(vec![]);
        self.name_index.entry(name.to_string()).or_default().push(id);
        id
    }

    pub fn get_or_add_route(&mut self, name: &str) -> usize {
        if let Some(&id) = self.route_ids.get(name) { id }
        else { let id = self.route_names.len(); self.route_names.push(name.to_string()); self.route_ids.insert(name.to_string(), id); id }
    }

    pub fn add_undirected_edge(&mut self, v1: usize, v2: usize, route_id: usize) {
        let d = distance_nm(self.vertices[v1].latitude, self.vertices[v1].longitude, self.vertices[v2].latitude, self.vertices[v2].longitude);
        self.edges[v1].push(Edge { target: v2, distance: d, route_id });
        self.edges[v2].push(Edge { target: v1, distance: d, route_id });
    }

    pub fn add_directed_edge(&mut self, from: usize, to: usize, route_id: usize) {
        let d = distance_nm(self.vertices[from].latitude, self.vertices[from].longitude, self.vertices[to].latitude, self.vertices[to].longitude);
        self.edges[from].push(Edge { target: to, distance: d, route_id });
    }

    pub fn find_vertex_ids(&self, name: &str) -> Vec<usize> { self.name_index.get(name).cloned().unwrap_or_default() }
    pub fn find_first_id(&self, name: &str) -> Option<usize> { self.name_index.get(name).and_then(|v| v.first().copied()) }

    pub fn a_star(&self, start: usize, target: usize) -> (f64, Vec<usize>) {
        let n = self.vertices.len();
        let target_pos = self.vertices[target].pos3;
        let inf = f64::INFINITY;
        let mut g = vec![inf; n];
        let mut prev = vec![usize::MAX; n];
        let mut visited = vec![false; n];
        g[start] = 0.0;
        let sh = self.vertices[start].pos3.chord_nm(&target_pos);
        let mut pq = BinaryHeap::new();
        pq.push(OrderedF64(-sh, start));

        while let Some(OrderedF64(_, u)) = pq.pop() {
            if u == target { return (g[target], reconstruct_path(start, target, &prev)); }
            if visited[u] { continue; }
            visited[u] = true;
            for e in &self.edges[u] {
                let v = e.target;
                if visited[v] { continue; }
                let ng = g[u] + e.distance;
                if ng < g[v] {
                    g[v] = ng; prev[v] = u;
                    let h = self.vertices[v].pos3.chord_nm(&target_pos);
                    pq.push(OrderedF64(-(ng + h), v));
                }
            }
        }
        (inf, vec![])
    }

    pub fn extract_legs(&self, path: &[usize], dep_vid: usize, arr_vid: usize) -> Vec<(String,String,String,f64)> {
        let mut legs = vec![];
        for i in 0..path.len().saturating_sub(1) {
            let (u, v) = (path[i], path[i+1]);
            let (mut br, mut bd) = (String::new(), 0.0);
            for e in &self.edges[u] {
                if e.target == v {
                    let rn = &self.route_names[e.route_id];
                    bd = e.distance;
                    if rn != "SID" && rn != "STAR" { br = rn.clone(); break; }
                    else { br = rn.clone(); }
                }
            }
            if br.is_empty() {
                if u == dep_vid { br = "SID".into(); }
                else if v == arr_vid { br = "STAR".into(); }
                else { br = "DCT".into(); }
            }
            legs.push((self.vertices[u].name.clone(), self.vertices[v].name.clone(), br, bd));
        }
        legs
    }
}

fn reconstruct_path(start: usize, target: usize, prev: &[usize]) -> Vec<usize> {
    if prev[target] == usize::MAX && start != target { return vec![]; }
    let mut p = vec![]; let mut c = target;
    loop { p.push(c); if c == start { break; } c = prev[c]; }
    p.reverse(); p
}

#[derive(PartialEq)]
struct OrderedF64(f64, usize);
impl Eq for OrderedF64 {}
impl PartialOrd for OrderedF64 { fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { self.0.partial_cmp(&o.0) } }
impl Ord for OrderedF64 { fn cmp(&self, o: &Self) -> std::cmp::Ordering { self.partial_cmp(o).unwrap_or(std::cmp::Ordering::Equal) } }
