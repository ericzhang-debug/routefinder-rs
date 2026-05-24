mod database;
mod graph;
mod finder;
mod models;

use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;
use finder::RouteFinder;
use models::{AirportDetail, ProcedureInfo};

const DB_PATH: &str = "./Navdata/nd.db3";
const W: usize = 72; // 统一输出宽度

fn sep(ch: char) -> String { ch.to_string().repeat(W) }
fn title(text: &str) { println!("\n{}", sep('─')); println!("  {text}"); println!("{}", sep('─')); }
fn hdr(text: &str) { println!("\n{}", sep('═')); println!("  {text}"); println!("{}", sep('═')); }

fn main() {
    let db_path = Path::new(DB_PATH);
    if !db_path.exists() {
        eprintln!("\n[ERROR] 导航数据库未找到: {}", DB_PATH);
        std::process::exit(1);
    }
    println!("加载导航数据...");
    let t0 = Instant::now();
    let mut finder = match RouteFinder::new(db_path) {
        Ok(f) => f,
        Err(e) => { eprintln!("  [ERROR] {e}"); std::process::exit(1); }
    };
    if let Err(e) = finder.initialize() {
        eprintln!("  [ERROR] 初始化失败: {e}");
        std::process::exit(1);
    }

    if let Ok(db) = database::NavDatabase::open(db_path) {
        if let Ok(cfg) = db.fetch_config() {
            println!("\n  导航数据库  Cycle {}    {} — {}",
                cfg.cycle_name, cfg.cycle_start, cfg.cycle_end);
        }
    }
    println!("  初始化耗时 {:.1}s", t0.elapsed().as_secs_f64());

    println!("\n  输入 ICAO 码查询航路 (q 退出)\n");

    loop {
        print!("  出发机场: "); io::stdout().flush().unwrap();
        let mut dep = String::new(); io::stdin().read_line(&mut dep).unwrap();
        let dep = dep.trim().to_string();
        if dep.eq_ignore_ascii_case("q") { break; }

        print!("  到达机场: "); io::stdout().flush().unwrap();
        let mut arr = String::new(); io::stdin().read_line(&mut arr).unwrap();
        let arr = arr.trim().to_string();
        if arr.eq_ignore_ascii_case("q") { break; }

        if dep.is_empty() || arr.is_empty() { println!("  [!] 请输入有效 ICAO 码\n"); continue; }

        println!("\n  {} → {} ...", dep.to_uppercase(), arr.to_uppercase());
        let t0 = Instant::now();
        match finder.find_route(&dep, &arr) {
            Ok(r) => {
                println!("  耗时 {:.1}s", t0.elapsed().as_secs_f64());
                print_result(&r);
            }
            Err(e) => println!("  [ERROR] {e}\n"),
        }
    }
    println!("  Bye!\n");
}

// ═══════════════════════════════════════════════════════════════
// 结果输出
// ═══════════════════════════════════════════════════════════════

fn print_result(r: &models::RouteResult) {
    // ── 机场 ──
    if let Some(ref info) = r.departure_info { print_airport(info, "出发"); }
    if let Some(ref info) = r.arrival_info { print_airport(info, "到达"); }

    // ── 航线 ──
    hdr(&format!("航线  {}", r.route));

    let total: f64 = r.legs.iter().map(|l| l.distance_nm).sum();
    println!("\n  {:<6} {:<6} {:<8} {:>8}   {:<6} {:<6} {:<8} {:>8}",
        "From", "To", "Via", "NM", "From", "To", "Via", "NM");
    println!("  {}", sep('─'));
    for leg in &r.legs {
        println!("  {:<6} {:<6} {:<8} {:>8.1}",
            leg.from_name, leg.to_name, leg.route, leg.distance_nm);
    }
    println!("  {}", sep('─'));
    println!("  {:<22} {:>8.1} NM", "TOTAL", total);

    // ── 航路点 ──
    let dep_name = r.departure_info.as_ref().map(|a| a.name.as_str()).unwrap_or("");
    let arr_name = r.arrival_info.as_ref().map(|a| a.name.as_str()).unwrap_or("");
    let dep_icao = r.departure_info.as_ref().map(|a| a.icao.as_str()).unwrap_or("");
    let arr_icao = r.arrival_info.as_ref().map(|a| a.icao.as_str()).unwrap_or("");

    println!("\n  ── 航路点 ──");
    println!("  {:<4} {:<8} {:<20} {:>12} {:>12}", "#", "Ident", "Name", "Lat", "Lon");
    println!("  {}", sep('─'));
    for (i, wp) in r.waypoints.iter().enumerate() {
        let dname = waypoint_display(wp, i, r.waypoints.len(), dep_icao, dep_name, arr_icao, arr_name);
        println!("  {:>3}  {:<8} {:<20} {:>12.6} {:>12.6}",
            i + 1, wp.name, dname, wp.latitude, wp.longitude);
    }

    // ── 程序 ──
    if !r.suggested_sids.is_empty() {
        title(&format!("建议离场程序  ({})", r.suggested_sids.len()));
        print_procs(&r.suggested_sids);
    }
    if !r.suggested_stars.is_empty() {
        title(&format!("建议进场程序  ({})", r.suggested_stars.len()));
        print_procs(&r.suggested_stars);
    }
    if !r.all_sids.is_empty() {
        title(&format!("{dep_icao} 全部离场程序  ({})", r.all_sids.len()));
        print_procs(&r.all_sids);
    }
    if !r.all_stars.is_empty() {
        title(&format!("{arr_icao} 全部进场程序  ({})", r.all_stars.len()));
        print_procs(&r.all_stars);
    }

    println!();
}

fn waypoint_display(wp: &models::Waypoint, i: usize, len: usize,
                    dep_icao: &str, dep_name: &str,
                    arr_icao: &str, arr_name: &str) -> String {
    if i == 0 && wp.name == dep_icao { return dep_name.into(); }
    if i == len - 1 && wp.name == arr_icao { return arr_name.into(); }
    if !wp.display_name.is_empty() { return wp.display_name.clone(); }
    String::new()
}

// ═══════════════════════════════════════════════════════════════
// 机场信息
// ═══════════════════════════════════════════════════════════════

fn print_airport(info: &AirportDetail, label: &str) {
    let (lat_s, lon_s) = database::lat_lon_label(info.latitude, info.longitude);
    title(&format!("{label}机场  {icao} — {name}", icao = info.icao, name = info.name));
    println!("  位置:       {lat_s}  {lon_s}");
    println!("  标高:       {} ft", info.elevation_ft);
    println!("  过渡高度:   {} ft      过渡高度层: {} ft",
        info.transition_altitude, info.transition_level);
    if info.speed_limit > 0 {
        println!("  速度限制:   {} kt ({} ft 以下)", info.speed_limit, info.speed_limit_altitude);
    }

    // 跑道
    if !info.runways.is_empty() {
        println!("\n  ── 跑道 ──");
        println!("  {:<6} {:>6}  {:<14} {:<6} {:>5}   {:<6} {:>8} {:>6}  {}",
            "RWY", "HDG°", "Size (ft)", "表面", "标高", "ILS", "频率", "类别", "DME");
        println!("  {}", sep('─'));
        for rwy in &info.runways {
            if let Some(ils) = info.ils.iter().find(|i| i.runway_ident == rwy.ident) {
                let dme = if ils.has_dme { "✓" } else { "—" };
                println!("  {:<6} {:>5.0}°  {:<5}x{:<8} {:<6} {:>4}ft   {:<6} {:>7}  Cat.{}   {dme}",
                    rwy.ident, rwy.true_heading,
                    rwy.length_ft, rwy.width_ft,
                    rwy.surface, rwy.elevation_ft,
                    ils.ident, ils.freq_mhz, ils.category);
            } else {
                println!("  {:<6} {:>5.0}°  {:<5}x{:<8} {:<6} {:>4}ft",
                    rwy.ident, rwy.true_heading,
                    rwy.length_ft, rwy.width_ft,
                    rwy.surface, rwy.elevation_ft);
            }
        }
    }

    // ATC
    if !info.communications.is_empty() {
        println!("\n  ── 通信频率 ──");
        println!("  {:<8} {:>10}  {}", "类型", "频率", "呼号");
        println!("  {}", sep('─'));
        let order = ["ATIS", "CLR", "GND", "TWR", "APP", "DEP", "CTR", "FSS", "UNI"];
        for ctype in &order {
            for c in &info.communications {
                if &c.comm_type == ctype {
                    let si = if c.service_indicator.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", c.service_indicator.trim())
                    };
                    println!("  {:<8} {:>8.3} MHz  {}{si}", c.comm_type, c.frequency_mhz, c.callsign);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// 程序列表
// ═══════════════════════════════════════════════════════════════

fn print_procs(procs: &[ProcedureInfo]) {
    let has_rwy = procs.iter().any(|p| !p.runway.is_empty());

    if has_rwy {
        println!("  {:<12} {:<5} {:<7} {:<10} {:>8} {:>5} {:>4} {:<16}  Fixes",
            "程序", "跑道", "导航台", "类型", "频率", "GS", "DME", "跑道尺寸");
    } else {
        println!("  {:<12} {:<7} {:<10} {:>8}  Fixes", "程序", "导航台", "类型", "频率");
    }
    println!("  {}", sep('─'));

    for p in procs {
        let fixes_str = p.fixes.join(" → ");

        let (nav_ident, nav_type, nav_freq, nav_gs, nav_dme) = nav_info(p);

        if has_rwy {
            let size = if p.runway_length_ft > 0 {
                format!("{}x{}ft", p.runway_length_ft, p.runway_width_ft)
            } else { "—".into() };
            let rwy = if p.runway.is_empty() { "—".into() } else { p.runway.clone() };
            println!("  {:<12} {:<5} {:<7} {:<10} {:>8} {:>5} {:>4} {:<16}  {fixes_str}",
                p.name, rwy, nav_ident, nav_type, nav_freq, nav_gs, nav_dme, size);
        } else {
            println!("  {:<12} {:<7} {:<10} {:>8}  {fixes_str}",
                p.name, nav_ident, nav_type, nav_freq);
        }
    }
}

fn nav_info(p: &ProcedureInfo) -> (String, String, String, String, String) {
    if !p.ils_ident.is_empty() {
        let cat = if !p.ils_category.is_empty() {
            format!("ILS Cat.{}", p.ils_category)
        } else { "ILS".into() };
        let gs = if p.ils_gs_angle > 0.0 { format!("{:.1}°", p.ils_gs_angle) } else { "—".into() };
        let dme = if p.ils_has_dme { "+".into() } else { String::new() };
        (p.ils_ident.clone(), cat, p.ils_freq_mhz.clone(), gs, dme)
    } else if let Some(nr) = p.nav_refs.first() {
        (nr.ident.clone(), nr.type_desc.clone(), nr.freq_mhz.clone(), "—".into(), String::new())
    } else {
        ("—".into(), "—".into(), "—".into(), "—".into(), String::new())
    }
}
