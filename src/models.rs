use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Leg {
    pub from_name: String,
    pub to_name: String,
    pub route: String,
    pub distance_nm: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Waypoint {
    pub ident: String,
    pub name: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NavConfig {
    pub cycle_name: String,
    pub cycle_start: String,
    pub cycle_end: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommRecord {
    pub comm_type: String,
    pub frequency_mhz: f64,
    pub callsign: String,
    pub service_indicator: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunwayRecord {
    pub ident: String,
    pub true_heading: f64,
    pub length_ft: i32,
    pub width_ft: i32,
    pub surface: String,
    pub latitude: f64,
    pub longitude: f64,
    pub elevation_ft: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IlsRecord {
    pub ident: String,
    pub freq_mhz: String,
    pub loc_course: f64,
    pub gs_angle: f64,
    pub category: String,
    pub has_dme: bool,
    pub runway_ident: String,
    pub elevation_ft: i32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AirportInfo {
    pub icao: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub elevation_ft: i32,
    pub transition_altitude: i32,
    pub transition_level: i32,
    pub speed_limit: i32,
    pub speed_limit_altitude: i32,
    pub runways: Vec<RunwayRecord>,
    pub ils: Vec<IlsRecord>,
    pub communications: Vec<CommRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavRef {
    pub ident: String,
    pub type_desc: String,
    pub freq_mhz: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProcedureInfo {
    pub name: String,
    pub full_name: String,
    pub proc_type: String,
    pub fixes: Vec<String>,
    pub nav_refs: Vec<NavRef>,
    pub runway: String,
    pub runway_heading: f64,
    pub runway_length_ft: i32,
    pub runway_width_ft: i32,
    pub runway_surface: String,
    pub ils_ident: String,
    pub ils_freq_mhz: String,
    pub ils_category: String,
    pub ils_gs_angle: f64,
    pub ils_has_dme: bool,
    pub ils_loc_course: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WaypointInfo {
    pub ident: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub navaid_type: String,
    pub navaid_name: String,
    pub frequency_mhz: String,
    pub navaid_range_nm: i32,
    pub elevation_ft: i32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AirwayWaypoint {
    pub ident: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AirwayInfo {
    pub airway: String,
    pub waypoints: Vec<AirwayWaypoint>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RouteResult {
    pub route: String,
    pub legs: Vec<Leg>,
    pub waypoints: Vec<Waypoint>,
    pub total_distance_nm: f64,
    pub departure: Option<AirportInfo>,
    pub arrival: Option<AirportInfo>,
    pub nav_config: Option<NavConfig>,
    pub suggested_sids: Vec<ProcedureInfo>,
    pub suggested_stars: Vec<ProcedureInfo>,
    pub all_sids: Vec<ProcedureInfo>,
    pub all_stars: Vec<ProcedureInfo>,
}
