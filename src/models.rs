//! 共享数据类型定义。

/// 航段信息。
#[derive(Debug, Clone)]
pub struct Leg {
    pub from_name: String,
    pub to_name: String,
    pub route: String,
    pub distance_nm: f64,
}

/// 航路点信息。
#[derive(Debug, Clone)]
pub struct Waypoint {
    pub name: String,
    pub display_name: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// 导航数据库版本信息。
#[derive(Debug, Clone, Default)]
pub struct NavConfig {
    pub cycle_name: String,
    pub cycle_start: String,
    pub cycle_end: String,
}

/// ATC 通信频率。
#[derive(Debug, Clone)]
pub struct CommRecord {
    pub comm_type: String,
    pub frequency_mhz: f64,
    pub callsign: String,
    pub service_indicator: String,
}

/// 跑道信息。
#[derive(Debug, Clone)]
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

/// ILS 信息。
#[derive(Debug, Clone)]
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

/// 机场完整信息。
#[derive(Debug, Clone, Default)]
pub struct AirportDetail {
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

/// 导航台引用。
#[derive(Debug, Clone)]
pub struct NavRef {
    pub ident: String,
    pub type_desc: String,
    pub freq_mhz: String,
}

/// 离场/进场程序信息。
#[derive(Debug, Clone, Default)]
pub struct ProcedureInfo {
    pub name: String,
    pub full_name: String,
    pub proc_type: String,      // "SID" or "STAR"
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

/// 航路查询结果。
#[derive(Debug, Default)]
pub struct RouteResult {
    pub route: String,
    pub legs: Vec<Leg>,
    pub waypoints: Vec<Waypoint>,
    pub departure_info: Option<AirportDetail>,
    pub arrival_info: Option<AirportDetail>,
    pub nav_config: Option<NavConfig>,
    pub suggested_sids: Vec<ProcedureInfo>,
    pub suggested_stars: Vec<ProcedureInfo>,
    pub all_sids: Vec<ProcedureInfo>,
    pub all_stars: Vec<ProcedureInfo>,
}
