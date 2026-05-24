use rusqlite::{Connection, params};
use std::path::Path;
use crate::models::*;

/// BCD32 编码频率 → MHz 字符串 (如 "114.70")
pub fn decode_bcd_freq(raw: i32) -> String {
    let b3 = ((raw >> 24) & 0xFF) as u8;
    let b2 = ((raw >> 16) & 0xFF) as u8;
    let b1 = ((raw >> 8) & 0xFF) as u8;

    let bcd = |b: u8| -> String {
        format!("{}{}", (b >> 4) & 0xF, b & 0xF)
    };

    let digits = format!("{}{}{}", bcd(b3), bcd(b2), bcd(b1));
    // digits[0]=flag, digits[1..]=freq
    if digits.len() >= 6 {
        let freq_digits = &digits[1..];
        format!("{}.{}", &freq_digits[..3], &freq_digits[3..5])
    } else {
        digits
    }
}

/// 度数 → 度分标签 (如 "N040°04.4")
pub fn lat_lon_label(lat: f64, lon: f64) -> (String, String) {
    fn fmt_dm(val: f64, pos: &str, neg: &str) -> String {
        let d = val.abs();
        let deg = d as i32;
        let min = (d - deg as f64) * 60.0;
        format!("{}{:03}°{:04.1}", if val >= 0.0 { pos } else { neg }, deg, min)
    }
    (fmt_dm(lat, "N", "S"), fmt_dm(lon, "E", "W"))
}

/// 数据库访问层。
pub struct NavDatabase {
    conn: Connection,
}

impl NavDatabase {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    // ── 索引 ──────────────────────────────────────────────

    pub fn create_indices(&self) -> rusqlite::Result<()> {
        let indices = [
            "CREATE INDEX IF NOT EXISTS idx_terminallegs_terminal ON TerminalLegs(TerminalID)",
            "CREATE INDEX IF NOT EXISTS idx_terminallegs_wpt      ON TerminalLegs(WptID)",
            "CREATE INDEX IF NOT EXISTS idx_terminallegs_nav      ON TerminalLegs(NavID)",
            "CREATE INDEX IF NOT EXISTS idx_terminals_airport     ON Terminals(AirportID)",
            "CREATE INDEX IF NOT EXISTS idx_terminals_rwy         ON Terminals(RwyID)",
            "CREATE INDEX IF NOT EXISTS idx_terminals_proc        ON Terminals(Proc)",
            "CREATE INDEX IF NOT EXISTS idx_terminals_icao        ON Terminals(ICAO)",
            "CREATE INDEX IF NOT EXISTS idx_runways_airport       ON Runways(AirportID)",
            "CREATE INDEX IF NOT EXISTS idx_ilses_runway          ON ILSes(RunwayID)",
            "CREATE INDEX IF NOT EXISTS idx_airwaylegs_airway     ON AirwayLegs(AirwayID)",
            "CREATE INDEX IF NOT EXISTS idx_airwaylegs_wpt1       ON AirwayLegs(Waypoint1ID)",
            "CREATE INDEX IF NOT EXISTS idx_airwaylegs_wpt2       ON AirwayLegs(Waypoint2ID)",
        ];
        for sql in &indices {
            self.conn.execute(sql, [])?;
        }
        Ok(())
    }

    // ── 配置 ──────────────────────────────────────────────

    pub fn fetch_config(&self) -> rusqlite::Result<NavConfig> {
        let mut cfg = NavConfig::default();
        let mut stmt = self.conn.prepare("SELECT key, val FROM config")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (k, v) = row?;
            match k.as_str() {
                "CycleName" => cfg.cycle_name = v,
                "CycleStartDate" => cfg.cycle_start = v,
                "CycleEndDate" => cfg.cycle_end = v,
                _ => {}
            }
        }
        Ok(cfg)
    }

    // ── 机场 ──────────────────────────────────────────────

    pub fn fetch_all_airports(&self) -> rusqlite::Result<Vec<(i32, String, f64, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT ID, ICAO, Latitude, Longtitude FROM Airports WHERE ICAO IS NOT NULL AND ICAO != ''"
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,  // ID
                r.get::<_, String>(1)?.to_uppercase(),
                r.get(2)?,  // Latitude
                r.get(3)?,  // Longtitude
            ))
        })?;
        rows.collect()
    }

    pub fn find_airport_id_by_icao(&self, icao: &str) -> rusqlite::Result<Option<i32>> {
        let mut stmt = self.conn.prepare(
            "SELECT ID FROM Airports WHERE ICAO = ?1"
        )?;
        Ok(stmt.query_row(params![icao.to_uppercase()], |r| r.get(0)).ok())
    }

    pub fn fetch_airport_detail(&self, icao: &str) -> rusqlite::Result<Option<AirportDetail>> {
        let mut stmt = self.conn.prepare(
            "SELECT ID, ICAO, Name, Latitude, Longtitude, Elevation, \
             TransitionAltitude, TransitionLevel, SpeedLimit, SpeedLimitAltitude \
             FROM Airports WHERE ICAO = ?1"
        )?;
        let row = stmt.query_row(params![icao.to_uppercase()], |r| {
            Ok(AirportDetail {
                icao: r.get::<_, String>(1)?.to_uppercase(),
                name: r.get::<_, String>(2).unwrap_or_default(),
                latitude: r.get(3)?,
                longitude: r.get(4)?,
                elevation_ft: r.get(5).unwrap_or(0),
                transition_altitude: r.get(6).unwrap_or(0),
                transition_level: r.get(7).unwrap_or(0),
                speed_limit: r.get(8).unwrap_or(0),
                speed_limit_altitude: r.get(9).unwrap_or(0),
                runways: vec![],
                ils: vec![],
                communications: vec![],
            })
        });

        match row {
            Ok(mut ap) => {
                let airport_id: i32 = self.conn.query_row(
                    "SELECT ID FROM Airports WHERE ICAO = ?1",
                    params![icao.to_uppercase()],
                    |r| r.get(0),
                )?;

                // 跑道
                let mut stmt2 = self.conn.prepare(
                    "SELECT Ident, TrueHeading, Length, Width, Surface, \
                     Latitude, Longtitude, Elevation \
                     FROM Runways WHERE AirportID = ?1 ORDER BY Ident"
                )?;
                let runways: Vec<RunwayRecord> = stmt2.query_map(
                    params![airport_id],
                    |r| Ok(RunwayRecord {
                        ident: r.get::<_, String>(0).unwrap_or_default(),
                        true_heading: r.get(1).unwrap_or(0.0),
                        length_ft: r.get(2).unwrap_or(0),
                        width_ft: r.get(3).unwrap_or(0),
                        surface: r.get::<_, String>(4).unwrap_or_default(),
                        latitude: r.get(5).unwrap_or(0.0),
                        longitude: r.get(6).unwrap_or(0.0),
                        elevation_ft: r.get(7).unwrap_or(0),
                    })
                )?.collect::<rusqlite::Result<Vec<_>>>()?;
                ap.runways = runways;

                // ILS
                let mut stmt3 = self.conn.prepare(
                    "SELECT i.Ident, i.Freq, i.LocCourse, i.GsAngle, i.Category, \
                            i.HasDme, r.Ident, i.Elevation \
                     FROM ILSes i JOIN Runways r ON i.RunwayID = r.ID \
                     WHERE r.AirportID = ?1 ORDER BY i.Ident"
                )?;
                let ils_list: Vec<IlsRecord> = stmt3.query_map(
                    params![airport_id],
                    |r| Ok(IlsRecord {
                        ident: r.get::<_, String>(0).unwrap_or_default(),
                        freq_mhz: decode_bcd_freq(r.get(1).unwrap_or(0)),
                        loc_course: r.get(2).unwrap_or(0.0),
                        gs_angle: r.get(3).unwrap_or(0.0),
                        category: r.get::<_, String>(4).unwrap_or_default(),
                        has_dme: r.get::<_, i32>(5).unwrap_or(0) != 0,
                        runway_ident: r.get::<_, String>(6).unwrap_or_default(),
                        elevation_ft: r.get(7).unwrap_or(0),
                    })
                )?.collect::<rusqlite::Result<Vec<_>>>()?;
                ap.ils = ils_list;

                // ATC
                let mut stmt4 = self.conn.prepare(
                    "SELECT communication_type, communication_frequency, \
                            callsign, service_indicator \
                     FROM AirportCommunication \
                     WHERE airport_identifier = ?1 OR icao_code = ?1 \
                     ORDER BY communication_type, communication_frequency"
                )?;
                let comms: Vec<CommRecord> = stmt4.query_map(
                    params![icao.to_uppercase()],
                    |r| Ok(CommRecord {
                        comm_type: r.get::<_, String>(0).unwrap_or_default(),
                        frequency_mhz: r.get(1).unwrap_or(0.0),
                        callsign: r.get::<_, String>(2).unwrap_or_default(),
                        service_indicator: r.get::<_, String>(3).unwrap_or_default(),
                    })
                )?.collect::<rusqlite::Result<Vec<_>>>()?;
                ap.communications = comms;

                Ok(Some(ap))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ── 航路点 ────────────────────────────────────────────

    pub fn fetch_all_waypoints(&self) -> rusqlite::Result<Vec<(i32, String, f64, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT ID, Ident, Latitude, Longtitude FROM Waypoints"
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, String>(1)?,
                r.get(2)?,
                r.get(3)?,
            ))
        })?;
        rows.collect()
    }

    /// 获取航路点名称（Name 字段，若与 Ident 不同则返回）。
    pub fn fetch_waypoint_display_name(&self, ident: &str) -> rusqlite::Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT Name FROM Waypoints WHERE Ident = ?1 LIMIT 1"
        )?;
        match stmt.query_row(params![ident], |r| r.get::<_, String>(0)) {
            Ok(name) if !name.is_empty() && name != ident => Ok(Some(name)),
            _ => Ok(None),
        }
    }

    // ── 航路 ──────────────────────────────────────────────

    pub fn fetch_all_airway_legs(
        &self,
    ) -> rusqlite::Result<Vec<(String, i32, i32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.Ident, al.Waypoint1ID, al.Waypoint2ID \
             FROM AirwayLegs al \
             JOIN Airways a ON al.AirwayID = a.ID \
             WHERE al.Waypoint1ID IS NOT NULL AND al.Waypoint2ID IS NOT NULL"
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get(1)?,
                r.get(2)?,
            ))
        })?;
        rows.collect()
    }

    // ── 终端区程序 ────────────────────────────────────────

    pub fn fetch_terminal_fixes_for_airport(
        &self, airport_id: i32,
    ) -> rusqlite::Result<(Vec<(i32, i32)>, Vec<(i32, i32)>)> {
        // 返回 (sid_links, star_links): (airport_db_id, wpt_db_id)
        let mut stmt = self.conn.prepare(
            "SELECT ID, Proc FROM Terminals WHERE AirportID = ?1"
        )?;
        let terms: Vec<(i32, String)> = stmt.query_map(
            params![airport_id],
            |r| Ok((r.get(0)?, r.get::<_, String>(1).unwrap_or_default()))
        )?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut sid_links = Vec::new();
        let mut star_links = Vec::new();

        let mut leg_stmt = self.conn.prepare(
            "SELECT WptID FROM TerminalLegs WHERE TerminalID = ?1 AND WptID IS NOT NULL"
        )?;

        for (term_id, proc) in &terms {
            let wpt_ids: Vec<i32> = leg_stmt.query_map(
                params![term_id],
                |r| r.get(0)
            )?.collect::<rusqlite::Result<Vec<_>>>()?;

            for wpt_id in wpt_ids {
                if proc == "1" {
                    sid_links.push((airport_id, wpt_id));
                } else if proc == "2" {
                    star_links.push((wpt_id, airport_id));
                }
            }
        }

        Ok((sid_links, star_links))
    }

    // ── 程序详情 ──────────────────────────────────────────

    pub fn find_procedures_by_wpt(
        &self, wpt_ident: &str, airport_icao: &str, proc_type: &str,
    ) -> rusqlite::Result<Vec<ProcedureInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT t.ID, t.FullName, t.Name, t.Rwy, t.RwyID \
             FROM TerminalLegs tl \
             JOIN Terminals t ON tl.TerminalID = t.ID \
             JOIN Waypoints w ON tl.WptID = w.ID \
             WHERE w.Ident = ?1 AND t.ICAO = ?2 AND t.Proc = ?3"
        )?;
        let rows = stmt.query_map(
            params![wpt_ident.to_uppercase(), airport_icao.to_uppercase(), proc_type],
            |r| Ok((
                r.get::<_, i32>(0)?,
                r.get::<_, String>(1).unwrap_or_default(),
                r.get::<_, String>(2).unwrap_or_default(),
                r.get::<_, String>(3).unwrap_or_default(),
                r.get::<_, Option<i32>>(4)?,
            ))
        )?;

        let mut results = Vec::new();
        for row in rows {
            let (term_id, full_name, name, _rwy_text, rwy_id) = row?;
            results.push(self.build_procedure_info(
                term_id, &full_name, &name, rwy_id, proc_type
            )?);
        }
        Ok(results)
    }

    pub fn fetch_all_procedures(
        &self, airport_icao: &str, proc_type: &str,
    ) -> rusqlite::Result<Vec<ProcedureInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.ID, t.FullName, t.Name, t.RwyID \
             FROM Terminals t \
             WHERE t.ICAO = ?1 AND t.Proc = ?2 \
             ORDER BY t.FullName"
        )?;
        let rows = stmt.query_map(
            params![airport_icao.to_uppercase(), proc_type],
            |r| Ok((
                r.get::<_, i32>(0)?,
                r.get::<_, String>(1).unwrap_or_default(),
                r.get::<_, String>(2).unwrap_or_default(),
                r.get::<_, Option<i32>>(3)?,
            ))
        )?;

        let mut results = Vec::new();
        for row in rows {
            let (term_id, full_name, name, rwy_id) = row?;
            results.push(self.build_procedure_info(
                term_id, &full_name, &name, rwy_id, proc_type
            )?);
        }
        Ok(results)
    }

    fn build_procedure_info(
        &self, term_id: i32, full_name: &str, name: &str,
        rwy_id: Option<i32>, proc_type: &str,
    ) -> rusqlite::Result<ProcedureInfo> {
        let mut info = ProcedureInfo {
            name: if name.is_empty() { full_name.to_string() } else { name.to_string() },
            full_name: full_name.to_string(),
            proc_type: if proc_type == "1" { "SID".into() } else { "STAR".into() },
            ..Default::default()
        };

        // 航路点序列
        let mut stmt = self.conn.prepare(
            "SELECT w.Ident FROM TerminalLegs tl \
             LEFT JOIN Waypoints w ON tl.WptID = w.ID \
             WHERE tl.TerminalID = ?1 AND tl.WptID IS NOT NULL \
             ORDER BY tl.ID"
        )?;
        let fixes: Vec<String> = stmt.query_map(
            params![term_id],
            |r| Ok(r.get::<_, String>(0).unwrap_or_default())
        )?.filter_map(|s| s.ok().filter(|s| !s.is_empty()))
        .collect();
        info.fixes = fixes;

        // 导航台引用
        let mut stmt2 = self.conn.prepare(
            "SELECT DISTINCT n.Ident, nt.Desc, n.Freq \
             FROM TerminalLegs tl \
             JOIN Navaids n ON tl.NavID = n.ID \
             JOIN NavaidTypes nt ON n.Type = nt.Type \
             WHERE tl.TerminalID = ?1 AND tl.NavID IS NOT NULL"
        )?;
        let navs: Vec<NavRef> = stmt2.query_map(
            params![term_id],
            |r| {
                let freq_raw: i32 = r.get(2).unwrap_or(0);
                Ok(NavRef {
                    ident: r.get::<_, String>(0).unwrap_or_default(),
                    type_desc: r.get::<_, String>(1).unwrap_or_default(),
                    freq_mhz: decode_bcd_freq(freq_raw),
                })
            }
        )?.collect::<rusqlite::Result<Vec<_>>>()?;
        info.nav_refs = navs;

        // 跑道 & ILS
        if let Some(rid) = rwy_id {
            let mut stmt3 = self.conn.prepare(
                "SELECT Ident, TrueHeading, Length, Width, Surface FROM Runways WHERE ID = ?1"
            )?;
            if let Ok(rwy_row) = stmt3.query_row(params![rid], |r| {
                Ok((
                    r.get::<_, String>(0).unwrap_or_default(),
                    r.get::<_, f64>(1).unwrap_or(0.0),
                    r.get::<_, i32>(2).unwrap_or(0),
                    r.get::<_, i32>(3).unwrap_or(0),
                    r.get::<_, String>(4).unwrap_or_default(),
                ))
            }) {
                info.runway = rwy_row.0;
                info.runway_heading = rwy_row.1;
                info.runway_length_ft = rwy_row.2;
                info.runway_width_ft = rwy_row.3;
                info.runway_surface = rwy_row.4;

                // ILS
                let mut stmt4 = self.conn.prepare(
                    "SELECT Ident, Freq, LocCourse, GsAngle, Category, HasDme \
                     FROM ILSes WHERE RunwayID = ?1 LIMIT 1"
                )?;
                if let Ok(ils_row) = stmt4.query_row(params![rid], |r| {
                    Ok((
                        r.get::<_, String>(0).unwrap_or_default(),
                        r.get::<_, i32>(1).unwrap_or(0),
                        r.get::<_, f64>(2).unwrap_or(0.0),
                        r.get::<_, f64>(3).unwrap_or(0.0),
                        r.get::<_, String>(4).unwrap_or_default(),
                        r.get::<_, i32>(5).unwrap_or(0) != 0,
                    ))
                }) {
                    info.ils_ident = ils_row.0;
                    info.ils_freq_mhz = decode_bcd_freq(ils_row.1);
                    info.ils_loc_course = ils_row.2;
                    info.ils_gs_angle = ils_row.3;
                    info.ils_category = ils_row.4;
                    info.ils_has_dme = ils_row.5;
                }
            }
        }

        Ok(info)
    }
}
