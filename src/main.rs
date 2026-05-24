mod database;
mod finder;
mod graph;
mod models;

use actix_web::{web, App, HttpServer, HttpResponse};
use serde_json::json;
use std::path::Path;
use finder::RouteFinder;
use database::Database;

const DB_PATH: &str = "./Navdata/nd.db3";

struct AppState {
    finder: RouteFinder,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db_path = Path::new(DB_PATH);
    if !db_path.exists() {
        eprintln!("[ERROR] 导航数据库未找到: {}", DB_PATH);
        std::process::exit(1);
    }

    println!("[Init] Opening database...");
    let db = Database::open(db_path).expect("Failed to open database");
    println!("[Init] Creating indices...");
    db.create_indices().expect("Failed to create indices");
    println!("[Init] Loading navigation data...");
    let finder = RouteFinder::new(db).expect("Failed to load navigation data");
    println!("[Init] Ready. Starting server on http://0.0.0.0:3001");

    let state = web::Data::new(AppState { finder });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            // ── 总查询 ──
            .route("/route", web::get().to(full_route))
            // ── 机场信息 ──
            .route("/airport/{icao}", web::get().to(airport_info))
            .route("/airport/{icao}/runways", web::get().to(airport_runways))
            .route("/airport/{icao}/communications", web::get().to(airport_communications))
            // ── 程序 ──
            .route("/airport/{icao}/procedures", web::get().to(airport_procedures))
            .route("/airport/{icao}/procedures/suggested", web::get().to(airport_suggested_procedures))
            // ── 离场 / 进场 ──
            .route("/airport/{icao}/departures", web::get().to(airport_departures))
            .route("/airport/{icao}/arrivals", web::get().to(airport_arrivals))
            // ── 航段 / 航路点 (子查询) ──
            .route("/route/text", web::get().to(route_text))
            .route("/route/legs", web::get().to(route_legs))
            .route("/route/waypoints", web::get().to(route_waypoints))
            // ── 航点 / 航路 ──
            .route("/waypoint/{ident}", web::get().to(waypoint_info))
            .route("/airway/{ident}", web::get().to(airway_info))
            // ── 元数据 ──
            .route("/navdata", web::get().to(navdata_info))
            // ── 健康检查 ──
            .route("/health", web::get().to(health))
            // ── 首页 ──
            .route("/", web::get().to(index))
    })
    .bind("0.0.0.0:3001")?
    .run()
    .await
}

// ═══════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════

#[derive(serde::Deserialize)]
struct RouteQuery { dep: String, arr: String }

async fn full_route(state: web::Data<AppState>, q: web::Query<RouteQuery>) -> HttpResponse {
    println!("[INFO] GET /route  dep={}  arr={}", q.dep.to_uppercase(), q.arr.to_uppercase());
    match state.finder.find_route(&q.dep, &q.arr) {
        Ok(r) => {
            println!("[INFO] /route  ->  {}  ({:.1} nm, {} legs)", r.route, r.total_distance_nm, r.legs.len());
            HttpResponse::Ok().json(r)
        }
        Err(e) => {
            eprintln!("[WARN] /route  ->  error: {}", e);
            HttpResponse::BadRequest().json(json!({"error": e}))
        }
    }
}

async fn airport_info(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let icao = path.into_inner();
    println!("[INFO] GET /airport/{}", icao.to_uppercase());
    match state.finder.get_airport_info(&icao) {
        Ok(Some(info)) => {
            println!("[INFO] /airport/{}  ->  {} ({} runways, {} ILS)", icao, info.name, info.runways.len(), info.ils.len());
            HttpResponse::Ok().json(info)
        }
        Ok(None) => {
            eprintln!("[WARN] /airport/{}  ->  not found", icao);
            HttpResponse::NotFound().json(json!({"error": format!("Airport '{}' not found", icao)}))
        }
        Err(e) => {
            eprintln!("[ERROR] /airport/{}  ->  {}", icao, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn airport_runways(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let icao = path.into_inner();
    println!("[INFO] GET /airport/{}/runways", icao.to_uppercase());
    match state.finder.get_airport_info(&icao) {
        Ok(Some(info)) => {
            println!("[INFO] /airport/{}/runways  ->  {} runways", icao, info.runways.len());
            HttpResponse::Ok().json(info.runways)
        }
        Ok(None) => {
            eprintln!("[WARN] /airport/{}/runways  ->  not found", icao);
            HttpResponse::NotFound().json(json!({"error": "Not found"}))
        }
        Err(e) => {
            eprintln!("[ERROR] /airport/{}/runways  ->  {}", icao, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn airport_communications(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let icao = path.into_inner();
    println!("[INFO] GET /airport/{}/communications", icao.to_uppercase());
    match state.finder.get_airport_info(&icao) {
        Ok(Some(info)) => {
            println!("[INFO] /airport/{}/communications  ->  {} frequencies", icao, info.communications.len());
            HttpResponse::Ok().json(info.communications)
        }
        Ok(None) => {
            eprintln!("[WARN] /airport/{}/communications  ->  not found", icao);
            HttpResponse::NotFound().json(json!({"error": "Not found"}))
        }
        Err(e) => {
            eprintln!("[ERROR] /airport/{}/communications  ->  {}", icao, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

#[derive(serde::Deserialize)]
struct ProceduresQuery { r#type: Option<String> }

async fn airport_procedures(state: web::Data<AppState>, path: web::Path<String>, q: web::Query<ProceduresQuery>) -> HttpResponse {
    let icao = path.into_inner();
    let ptype = q.r#type.as_deref().unwrap_or("1");
    let ptype_db = if ptype.eq_ignore_ascii_case("STAR") { "2" } else { "1" };
    let label = if ptype_db == "1" { "SID" } else { "STAR" };
    println!("[INFO] GET /airport/{}/procedures  type={}", icao.to_uppercase(), label);
    match state.finder.get_procedures(&icao, ptype_db) {
        Ok(procs) => {
            println!("[INFO] /airport/{}/procedures  ->  {} {}", icao, procs.len(), label);
            HttpResponse::Ok().json(procs)
        }
        Err(e) => {
            eprintln!("[ERROR] /airport/{}/procedures  ->  {}", icao, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

#[derive(serde::Deserialize)]
struct SuggestedQuery { wpt: String, r#type: Option<String> }

async fn airport_suggested_procedures(state: web::Data<AppState>, path: web::Path<String>, q: web::Query<SuggestedQuery>) -> HttpResponse {
    let icao = path.into_inner();
    let ptype = q.r#type.as_deref().unwrap_or("1");
    let ptype_db = if ptype.eq_ignore_ascii_case("STAR") { "2" } else { "1" };
    let label = if ptype_db == "1" { "SID" } else { "STAR" };
    println!("[INFO] GET /airport/{}/procedures/suggested  wpt={}  type={}", icao.to_uppercase(), q.wpt.to_uppercase(), label);
    match state.finder.get_suggested_procedures(&icao, &q.wpt, ptype_db) {
        Ok(procs) => {
            println!("[INFO] /airport/{}/procedures/suggested  ->  {} matches", icao, procs.len());
            HttpResponse::Ok().json(procs)
        }
        Err(e) => {
            eprintln!("[ERROR] /airport/{}/procedures/suggested  ->  {}", icao, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn airport_departures(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let icao = path.into_inner();
    println!("[INFO] GET /airport/{}/departures", icao.to_uppercase());
    match state.finder.get_procedures(&icao, "1") {
        Ok(procs) => {
            println!("[INFO] /airport/{}/departures  ->  {} SIDs", icao, procs.len());
            HttpResponse::Ok().json(procs)
        }
        Err(e) => {
            eprintln!("[ERROR] /airport/{}/departures  ->  {}", icao, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn airport_arrivals(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let icao = path.into_inner();
    println!("[INFO] GET /airport/{}/arrivals", icao.to_uppercase());
    match state.finder.get_procedures(&icao, "2") {
        Ok(procs) => {
            println!("[INFO] /airport/{}/arrivals  ->  {} STARs", icao, procs.len());
            HttpResponse::Ok().json(procs)
        }
        Err(e) => {
            eprintln!("[ERROR] /airport/{}/arrivals  ->  {}", icao, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn route_text(state: web::Data<AppState>, q: web::Query<RouteQuery>) -> HttpResponse {
    println!("[INFO] GET /route/text  dep={}  arr={}", q.dep.to_uppercase(), q.arr.to_uppercase());
    match state.finder.find_route(&q.dep, &q.arr) {
        Ok(r) => {
            println!("[INFO] /route/text  ->  {}", r.route);
            HttpResponse::Ok().json(json!({"route": r.route}))
        }
        Err(e) => {
            eprintln!("[WARN] /route/text  ->  error: {}", e);
            HttpResponse::BadRequest().json(json!({"error": e}))
        }
    }
}

async fn route_legs(state: web::Data<AppState>, q: web::Query<RouteQuery>) -> HttpResponse {
    println!("[INFO] GET /route/legs  dep={}  arr={}", q.dep.to_uppercase(), q.arr.to_uppercase());
    match state.finder.find_route(&q.dep, &q.arr) {
        Ok(r) => {
            println!("[INFO] /route/legs  ->  {} legs", r.legs.len());
            HttpResponse::Ok().json(r.legs)
        }
        Err(e) => {
            eprintln!("[WARN] /route/legs  ->  error: {}", e);
            HttpResponse::BadRequest().json(json!({"error": e}))
        }
    }
}

async fn route_waypoints(state: web::Data<AppState>, q: web::Query<RouteQuery>) -> HttpResponse {
    println!("[INFO] GET /route/waypoints  dep={}  arr={}", q.dep.to_uppercase(), q.arr.to_uppercase());
    match state.finder.find_route(&q.dep, &q.arr) {
        Ok(r) => {
            println!("[INFO] /route/waypoints  ->  {} waypoints", r.waypoints.len());
            HttpResponse::Ok().json(r.waypoints)
        }
        Err(e) => {
            eprintln!("[WARN] /route/waypoints  ->  error: {}", e);
            HttpResponse::BadRequest().json(json!({"error": e}))
        }
    }
}

async fn waypoint_info(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let ident = path.into_inner();
    println!("[INFO] GET /waypoint/{}", ident.to_uppercase());
    match state.finder.get_waypoint_info(&ident) {
        Ok(infos) if !infos.is_empty() => {
            println!("[INFO] /waypoint/{}  ->  {} result(s)", ident, infos.len());
            HttpResponse::Ok().json(infos)
        }
        Ok(_) => {
            eprintln!("[WARN] /waypoint/{}  ->  not found", ident);
            HttpResponse::NotFound().json(json!({"error": "Waypoint not found"}))
        }
        Err(e) => {
            eprintln!("[ERROR] /waypoint/{}  ->  {}", ident, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn airway_info(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let ident = path.into_inner();
    println!("[INFO] GET /airway/{}", ident.to_uppercase());
    match state.finder.get_airway_waypoints(&ident) {
        Ok(Some(info)) => {
            println!("[INFO] /airway/{}  ->  {} waypoints", ident, info.waypoints.len());
            HttpResponse::Ok().json(info)
        }
        Ok(None) => {
            eprintln!("[WARN] /airway/{}  ->  not found", ident);
            HttpResponse::NotFound().json(json!({"error": "Airway not found"}))
        }
        Err(e) => {
            eprintln!("[ERROR] /airway/{}  ->  {}", ident, e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn navdata_info(state: web::Data<AppState>) -> HttpResponse {
    println!("[INFO] GET /navdata");
    match state.finder.get_nav_config() {
        Ok(cfg) => {
            println!("[INFO] /navdata  ->  cycle={}", cfg.cycle_name);
            HttpResponse::Ok().json(cfg)
        }
        Err(e) => {
            eprintln!("[ERROR] /navdata  ->  {}", e);
            HttpResponse::InternalServerError().json(json!({"error": e}))
        }
    }
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

async fn index() -> HttpResponse {
    println!("[INFO] GET /");
    let html = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<title>RouteFinder API</title>
<style>
body { font-family: monospace; max-width: 900px; margin: 40px auto; padding: 0 20px; background: #111; color: #ddd; }
h1 { color: #fff; border-bottom: 2px solid #444; padding-bottom: 10px; }
h2 { color: #aaa; }
.endpoint { background: #1a1a1a; border-left: 3px solid #5af; margin: 12px 0; padding: 10px 16px; border-radius: 4px; }
.method { color: #5af; font-weight: bold; }
.path { color: #fff; }
.desc { margin-top: 6px; color: #aaa; }
.param { color: #fa0; }
.example { color: #8a8; margin-top: 4px; }
.example a { color: #8a8; text-decoration: none; }
.example a:hover { text-decoration: underline; }
</style>
</head>
<body>
<h1>RouterFind API</h1>
<p>导航航路查询服务. 所有接口均为 GET 请求, Base URL: <code>http://localhost:3001</code></p>

<h2>航路查询</h2>
<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/route</span></div>
<div class="desc">查询两个机场之间的完整航路</div>
<div class="param">参数: dep (起飞机场ICAO), arr (到达机场ICAO)</div>
<div class="example">示例: <a href="/route?dep=ZBAA&arr=ZGGG">/route?dep=ZBAA&arr=ZGGG</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/route/text</span></div>
<div class="desc">仅返回航路文本字符串</div>
<div class="param">参数: dep, arr (同上)</div>
<div class="example">示例: <a href="/route/text?dep=ZBAA&arr=ZGGG">/route/text?dep=ZBAA&arr=ZGGG</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/route/legs</span></div>
<div class="desc">仅返回航段列表</div>
<div class="param">参数: dep, arr (同上)</div>
<div class="example">示例: <a href="/route/legs?dep=ZBAA&arr=ZGGG">/route/legs?dep=ZBAA&arr=ZGGG</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/route/waypoints</span></div>
<div class="desc">仅返回航路点列表</div>
<div class="param">参数: dep, arr (同上)</div>
<div class="example">示例: <a href="/route/waypoints?dep=ZBAA&arr=ZGGG">/route/waypoints?dep=ZBAA&arr=ZGGG</a></div>
</div>

<h2>航点 / 航路</h2>
<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/waypoint/{ident}</span></div>
<div class="desc">查询航点信息（经纬度、名称、导航台类型、频率等），无关联导航台时相关字段为空</div>
<div class="example">示例: <a href="/waypoint/DUGEB">/waypoint/DUGEB</a> | <a href="/waypoint/HOK">/waypoint/HOK</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airway/{ident}</span></div>
<div class="desc">查询航路的所有航路点（按顺序排列，含经纬度）</div>
<div class="example">示例: <a href="/airway/G212">/airway/G212</a> | <a href="/airway/A461">/airway/A461</a></div>
</div>

<h2>机场信息</h2>
<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airport/{icao}</span></div>
<div class="desc">查询机场完整信息（跑道、ILS、通讯频率等）</div>
<div class="param">路径参数: icao — 机场四字码</div>
<div class="example">示例: <a href="/airport/ZBAA">/airport/ZBAA</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airport/{icao}/runways</span></div>
<div class="desc">仅返回跑道信息</div>
<div class="example">示例: <a href="/airport/ZBAA/runways">/airport/ZBAA/runways</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airport/{icao}/communications</span></div>
<div class="desc">仅返回通讯频率</div>
<div class="example">示例: <a href="/airport/ZBAA/communications">/airport/ZBAA/communications</a></div>
</div>

<h2>程序</h2>
<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airport/{icao}/procedures</span></div>
<div class="desc">查询机场的所有进离场程序</div>
<div class="param">参数: type (可选, <code>SID</code> 或 <code>STAR</code>, 默认为 SID)</div>
<div class="example">示例: <a href="/airport/ZBAA/procedures?type=SID">/airport/ZBAA/procedures?type=SID</a> | <a href="/airport/ZGGG/procedures?type=STAR">/airport/ZGGG/procedures?type=STAR</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airport/{icao}/procedures/suggested</span></div>
<div class="desc">根据航路点推荐匹配的进离场程序</div>
<div class="param">参数: wpt (航路点标识), type (可选, 同上)</div>
<div class="example">示例: <a href="/airport/ZBAA/procedures/suggested?wpt=DUGEB&type=SID">/airport/ZBAA/procedures/suggested?wpt=DUGEB&type=SID</a></div>
</div>

<h2>离场 / 进场</h2>
<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airport/{icao}/departures</span></div>
<div class="desc">查询机场所有离场程序 (SID)，包含跑道、航路点、导航台、ILS 等信息</div>
<div class="example">示例: <a href="/airport/ZBAA/departures">/airport/ZBAA/departures</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/airport/{icao}/arrivals</span></div>
<div class="desc">查询机场所有进场程序 (STAR)，包含跑道、航路点、导航台、ILS 等信息</div>
<div class="example">示例: <a href="/airport/ZGGG/arrivals">/airport/ZGGG/arrivals</a></div>
</div>

<h2>元数据</h2>
<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/navdata</span></div>
<div class="desc">返回导航数据版本信息</div>
<div class="example">示例: <a href="/navdata">/navdata</a></div>
</div>

<div class="endpoint">
<div><span class="method">GET</span> <span class="path">/health</span></div>
<div class="desc">健康检查</div>
<div class="example">示例: <a href="/health">/health</a></div>
</div>
</body>
</html>"#;
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}
