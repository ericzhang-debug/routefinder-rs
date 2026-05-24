//! 航线图与 A* 最短路径搜索。

use std::collections::{BinaryHeap, HashMap};
use std::f64::consts::PI;

const EARTH_RADIUS_NM: f64 = 6378.137 / 1.852;

// ═══════════════════════════════════════════════════════════════
// 大圆距离 (保留用于边权重计算)
// ═══════════════════════════════════════════════════════════════

pub fn distance_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rad = |x: f64| x * PI / 180.0;
    let rlat1 = rad(lat1);
    let rlat2 = rad(lat2);
    let a = rlat1 - rlat2;
    let b = rad(lon1) - rad(lon2);
    let s = 2.0 * ((a / 2.0).sin().powi(2)
        + rlat1.cos() * rlat2.cos() * (b / 2.0).sin().powi(2))
        .sqrt()
        .asin();
    s * EARTH_RADIUS_NM
}

/// 初始方位角 (度)。用于判定 SID/STAR 方向。
pub fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rad = |x: f64| x * PI / 180.0;
    let rlat1 = rad(lat1);
    let rlat2 = rad(lat2);
    let dlon = rad(lon2 - lon1);
    let y = dlon.sin() * rlat2.cos();
    let x = rlat1.cos() * rlat2.sin() - rlat1.sin() * rlat2.cos() * dlon.cos();
    let mut bearing = y.atan2(x).to_degrees();
    if bearing < 0.0 {
        bearing += 360.0;
    }
    bearing
}

/// 方位差 (0..180°)
pub fn bearing_diff(b1: f64, b2: f64) -> f64 {
    let diff = (b1 - b2).abs();
    diff.min(360.0 - diff)
}

/// 3D 单位球坐标，用于快速弦长启发式。
#[derive(Debug, Clone, Copy)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    /// 弦长距离 (海里) — 精确下界 ≤ 大圆距离。
    #[inline]
    pub fn chord_distance_nm(&self, other: &Vec3) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt() * EARTH_RADIUS_NM
    }
}

pub fn lat_lon_to_vec3(lat: f64, lon: f64) -> Vec3 {
    let rad = |x: f64| x * PI / 180.0;
    let rlat = rad(lat);
    let rlon = rad(lon);
    Vec3 {
        x: rlat.cos() * rlon.cos(),
        y: rlat.cos() * rlon.sin(),
        z: rlat.sin(),
    }
}

// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Vertex {
    pub id: usize,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub pos3: Vec3, // 预计算 3D 坐标
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub target: usize,
    pub distance: f64,
    pub route_id: usize,
}

pub struct Graph {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Vec<Edge>>,
    pub name_index: HashMap<String, Vec<usize>>,
    pub route_ids: HashMap<String, usize>,
    pub route_names: Vec<String>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            name_index: HashMap::new(),
            route_ids: HashMap::new(),
            route_names: Vec::new(),
        }
    }

    pub fn add_vertex(&mut self, name: &str, lat: f64, lon: f64) -> usize {
        let id = self.vertices.len();
        self.vertices.push(Vertex {
            id,
            name: name.to_string(),
            latitude: lat,
            longitude: lon,
            pos3: lat_lon_to_vec3(lat, lon),
        });
        self.edges.push(Vec::new());
        self.name_index.entry(name.to_string()).or_default().push(id);
        id
    }

    pub fn get_or_add_route(&mut self, name: &str) -> usize {
        if let Some(&id) = self.route_ids.get(name) {
            id
        } else {
            let id = self.route_names.len();
            self.route_names.push(name.to_string());
            self.route_ids.insert(name.to_string(), id);
            id
        }
    }

    pub fn add_undirected_edge(&mut self, v1: usize, v2: usize, route_id: usize) {
        // 边权重: 用精确 Haversine 公式
        let dist = distance_nm(
            self.vertices[v1].latitude, self.vertices[v1].longitude,
            self.vertices[v2].latitude, self.vertices[v2].longitude,
        );
        self.edges[v1].push(Edge { target: v2, distance: dist, route_id });
        self.edges[v2].push(Edge { target: v1, distance: dist, route_id });
    }

    pub fn add_directed_edge(&mut self, from: usize, to: usize, route_id: usize) {
        let dist = distance_nm(
            self.vertices[from].latitude, self.vertices[from].longitude,
            self.vertices[to].latitude, self.vertices[to].longitude,
        );
        self.edges[from].push(Edge { target: to, distance: dist, route_id });
    }

    pub fn find_vertex_ids(&self, name: &str) -> Vec<usize> {
        self.name_index.get(name).cloned().unwrap_or_default()
    }

    pub fn find_first_id(&self, name: &str) -> Option<usize> {
        self.name_index.get(name).and_then(|v| v.first().copied())
    }

    // ── A* 搜索 (快速弦长启发式) ────────────────────────────

    pub fn a_star(&self, start: usize, target: usize) -> (f64, Vec<usize>, usize) {
        let n = self.vertices.len();
        let target_pos = self.vertices[target].pos3;
        let inf = f64::INFINITY;

        let mut g = vec![inf; n];
        let mut prev = vec![usize::MAX; n];
        let mut visited = vec![false; n];

        g[start] = 0.0;
        let start_h = self.vertices[start].pos3.chord_distance_nm(&target_pos);
        let mut pq = BinaryHeap::new();
        pq.push(OrderedF64(-start_h, start));

        let mut expanded = 0usize;

        while let Some(OrderedF64(_neg_f, u)) = pq.pop() {
            if u == target {
                return (g[target], Self::reconstruct_path(start, target, &prev), expanded);
            }
            if visited[u] {
                continue;
            }
            visited[u] = true;
            expanded += 1;

            for edge in &self.edges[u] {
                let v = edge.target;
                if visited[v] {
                    continue;
                }
                let new_g = g[u] + edge.distance;
                if new_g < g[v] {
                    g[v] = new_g;
                    prev[v] = u;
                    let h = self.vertices[v].pos3.chord_distance_nm(&target_pos);
                    pq.push(OrderedF64(-(new_g + h), v));
                }
            }
        }

        (inf, vec![], expanded)
    }

    fn reconstruct_path(start: usize, target: usize, prev: &[usize]) -> Vec<usize> {
        if prev[target] == usize::MAX && start != target {
            return vec![];
        }
        let mut path = Vec::new();
        let mut cur = target;
        loop {
            path.push(cur);
            if cur == start {
                break;
            }
            cur = prev[cur];
        }
        path.reverse();
        path
    }

    pub fn extract_legs(
        &self, path: &[usize], dep_vid: usize, arr_vid: usize,
    ) -> Vec<(String, String, String, f64)> {
        let mut legs = Vec::new();
        for i in 0..path.len().saturating_sub(1) {
            let u = path[i];
            let v = path[i + 1];
            let is_first = u == dep_vid;
            let is_last = v == arr_vid;

            let mut best_route = String::new();
            let mut best_dist = 0.0_f64;

            for edge in &self.edges[u] {
                if edge.target == v {
                    let rn = &self.route_names[edge.route_id];
                    best_dist = edge.distance;
                    if rn != "SID" && rn != "STAR" {
                        best_route = rn.clone();
                        break;
                    } else {
                        best_route = rn.clone();
                    }
                }
            }

            if best_route.is_empty() {
                if is_first {
                    best_route = "SID".into();
                } else if is_last {
                    best_route = "STAR".into();
                } else {
                    best_route = "DCT".into();
                }
            }

            legs.push((
                self.vertices[u].name.clone(),
                self.vertices[v].name.clone(),
                best_route,
                best_dist,
            ));
        }
        legs
    }
}

#[derive(PartialEq)]
struct OrderedF64(f64, usize);

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}
