use rusqlite::{Connection, params};
use std::path::Path;
use crate::models::*;

pub fn decode_bcd_freq(raw: i32) -> String {
    let bcd = |b: u8| format!("{}{}", (b >> 4) & 0xF, b & 0xF);
    let b3 = ((raw >> 24) & 0xFF) as u8;
    let b2 = ((raw >> 16) & 0xFF) as u8;
    let b1 = ((raw >> 8) & 0xFF) as u8;
    let digits = format!("{}{}{}", bcd(b3), bcd(b2), bcd(b1));
    if digits.len() >= 6 {
        let freq = &digits[1..];
        format!("{}.{}", &freq[..3], &freq[3..5])
    } else { digits }
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Ok(Self { conn: Connection::open(path)? })
    }

    pub fn create_indices(&self) -> rusqlite::Result<()> {
        for sql in [
            "CREATE INDEX IF NOT EXISTS idx_terminallegs_terminal ON TerminalLegs(TerminalID)",
            "CREATE INDEX IF NOT EXISTS idx_terminallegs_wpt ON TerminalLegs(WptID)",
            "CREATE INDEX IF NOT EXISTS idx_terminallegs_nav ON TerminalLegs(NavID)",
            "CREATE INDEX IF NOT EXISTS idx_terminals_airport ON Terminals(AirportID)",
            "CREATE INDEX IF NOT EXISTS idx_terminals_rwy ON Terminals(RwyID)",
            "CREATE INDEX IF NOT EXISTS idx_terminals_proc ON Terminals(Proc)",
            "CREATE INDEX IF NOT EXISTS idx_runways_airport ON Runways(AirportID)",
            "CREATE INDEX IF NOT EXISTS idx_ilses_runway ON ILSes(RunwayID)",
            "CREATE INDEX IF NOT EXISTS idx_airwaylegs_airway ON AirwayLegs(AirwayID)",
            "CREATE INDEX IF NOT EXISTS idx_airwaylegs_wpt1 ON AirwayLegs(Waypoint1ID)",
            "CREATE INDEX IF NOT EXISTS idx_airwaylegs_wpt2 ON AirwayLegs(Waypoint2ID)",
        ] { self.conn.execute(sql, [])?; }
        Ok(())
    }

    pub fn fetch_config(&self) -> rusqlite::Result<NavConfig> {
        let mut cfg = NavConfig::default();
        let mut stmt = self.conn.prepare("SELECT key, val FROM config")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?)))? {
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

    pub fn fetch_waypoint_info(&self, ident: &str) -> rusqlite::Result<Vec<WaypointInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT w.Ident, w.Name, w.Latitude, w.Longtitude, \
             n.Ident, nt.Desc, n.Name, n.Freq, n.Range, n.Elevation \
             FROM Waypoints w \
             LEFT JOIN Navaids n ON w.NavaidID = n.ID \
             LEFT JOIN NavaidTypes nt ON n.Type = nt.Type \
             WHERE w.Ident=?1"
        )?;
        let rows = stmt.query_map(params![ident.to_uppercase()], |r| {
            Ok(WaypointInfo {
                ident: r.get::<_,String>(0).unwrap_or_default(),
                name: r.get::<_,String>(1).unwrap_or_default(),
                latitude: r.get(2).unwrap_or(0.0),
                longitude: r.get(3).unwrap_or(0.0),
                navaid_type: r.get::<_,String>(5).unwrap_or_default(),
                navaid_name: r.get::<_,String>(6).unwrap_or_default(),
                frequency_mhz: decode_bcd_freq(r.get(7).unwrap_or(0)),
                navaid_range_nm: r.get(8).unwrap_or(0),
                elevation_ft: r.get(9).unwrap_or(0),
            })
        })?;
        rows.collect()
    }

    pub fn fetch_airway_waypoints(&self, airway: &str) -> rusqlite::Result<Option<AirwayInfo>> {
        let airway_id: i32 = match self.conn.query_row(
            "SELECT ID FROM Airways WHERE Ident=?1", params![airway.to_uppercase()], |r| r.get(0)
        ) {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e),
        };

        let mut stmt = self.conn.prepare(
            "SELECT al.Waypoint1ID, al.Waypoint2ID FROM AirwayLegs al WHERE al.AirwayID=?1"
        )?;
        let legs: Vec<(i32,i32)> = stmt.query_map(params![airway_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;

        if legs.is_empty() {
            return Ok(None);
        }

        // Build adjacency map and find start waypoint
        let mut next_map: std::collections::HashMap<i32,i32> = std::collections::HashMap::new();
        let mut wpt1_set: std::collections::HashSet<i32> = std::collections::HashSet::new();
        let mut wpt2_set: std::collections::HashSet<i32> = std::collections::HashSet::new();
        for (w1, w2) in &legs {
            next_map.insert(*w1, *w2);
            wpt1_set.insert(*w1);
            wpt2_set.insert(*w2);
        }

        // Start = wpt1 not appearing as any wpt2
        let start_id = wpt1_set.iter().find(|id| !wpt2_set.contains(id)).copied();

        // Traverse chain
        let mut ordered_ids = Vec::new();
        if let Some(mut cur) = start_id {
            ordered_ids.push(cur);
            while let Some(next) = next_map.get(&cur) {
                ordered_ids.push(*next);
                cur = *next;
            }
        } else {
            // Fallback: no clear start, just use wpt1 from each leg in order
            for (w1, _) in &legs {
                if !ordered_ids.contains(w1) { ordered_ids.push(*w1); }
            }
        }

        // Resolve waypoint ids to info
        let mut wpts = Vec::new();
        let mut wstmt = self.conn.prepare("SELECT Ident, Name, Latitude, Longtitude FROM Waypoints WHERE ID=?1")?;
        for id in ordered_ids {
            if let Ok(row) = wstmt.query_row(params![id], |r| {
                Ok(AirwayWaypoint {
                    ident: r.get::<_,String>(0).unwrap_or_default(),
                    name: r.get::<_,String>(1).unwrap_or_default(),
                    latitude: r.get(2).unwrap_or(0.0),
                    longitude: r.get(3).unwrap_or(0.0),
                })
            }) {
                wpts.push(row);
            }
        }

        Ok(Some(AirwayInfo { airway: airway.to_uppercase(), waypoints: wpts }))
    }

    pub fn fetch_all_airports(&self) -> rusqlite::Result<Vec<(i32, String, f64, f64)>> {
        let mut stmt = self.conn.prepare("SELECT ID, ICAO, Latitude, Longtitude FROM Airports WHERE ICAO IS NOT NULL AND ICAO != ''")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_,String>(1)?.to_uppercase(), r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn find_airport_id(&self, icao: &str) -> rusqlite::Result<Option<i32>> {
        Ok(self.conn.query_row("SELECT ID FROM Airports WHERE ICAO=?1", params![icao.to_uppercase()], |r| r.get(0)).ok())
    }

    pub fn fetch_all_waypoints(&self) -> rusqlite::Result<Vec<(i32, String, f64, f64)>> {
        let mut stmt = self.conn.prepare("SELECT ID, Ident, Latitude, Longtitude FROM Waypoints")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_,String>(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn fetch_all_airway_legs(&self) -> rusqlite::Result<Vec<(String, i32, i32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.Ident, al.Waypoint1ID, al.Waypoint2ID FROM AirwayLegs al \
             JOIN Airways a ON al.AirwayID = a.ID \
             WHERE al.Waypoint1ID IS NOT NULL AND al.Waypoint2ID IS NOT NULL"
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn fetch_terminal_fixes(&self, airport_id: i32) -> rusqlite::Result<(Vec<(i32,i32)>, Vec<(i32,i32)>)> {
        let mut sid = Vec::new();
        let mut star = Vec::new();
        let mut tstmt = self.conn.prepare("SELECT ID, Proc FROM Terminals WHERE AirportID=?1")?;
        let terms: Vec<(i32,String)> = tstmt.query_map(params![airport_id], |r| Ok((r.get(0)?, r.get::<_,String>(1).unwrap_or_default())))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut lstmt = self.conn.prepare("SELECT WptID FROM TerminalLegs WHERE TerminalID=?1 AND WptID IS NOT NULL")?;
        for (tid, proc) in &terms {
            let wpts: Vec<i32> = lstmt.query_map(params![tid], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            for w in wpts {
                if proc == "1" { sid.push((airport_id, w)); }
                else if proc == "2" { star.push((w, airport_id)); }
            }
        }
        Ok((sid, star))
    }

    pub fn fetch_airport_info(&self, icao: &str) -> rusqlite::Result<Option<AirportInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT ICAO, Name, Latitude, Longtitude, Elevation, TransitionAltitude, TransitionLevel, SpeedLimit, SpeedLimitAltitude FROM Airports WHERE ICAO=?1"
        )?;
        let row = match stmt.query_row(params![icao.to_uppercase()], |r| Ok(AirportInfo {
            icao: r.get::<_,String>(0)?.to_uppercase(),
            name: r.get::<_,String>(1).unwrap_or_default(),
            latitude: r.get(2)?, longitude: r.get(3)?,
            elevation_ft: r.get(4).unwrap_or(0),
            transition_altitude: r.get(5).unwrap_or(0),
            transition_level: r.get(6).unwrap_or(0),
            speed_limit: r.get(7).unwrap_or(0),
            speed_limit_altitude: r.get(8).unwrap_or(0),
            ..Default::default()
        })) {
            Ok(ap) => ap,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e),
        };

        let mut ap = row;
        let airport_id: i32 = self.conn.query_row("SELECT ID FROM Airports WHERE ICAO=?1", params![icao.to_uppercase()], |r| r.get(0))?;

        let mut rs = self.conn.prepare("SELECT Ident, TrueHeading, Length, Width, Surface, Latitude, Longtitude, Elevation FROM Runways WHERE AirportID=?1 ORDER BY Ident")?;
        ap.runways = rs.query_map(params![airport_id], |r| Ok(RunwayRecord {
            ident: r.get::<_,String>(0).unwrap_or_default(), true_heading: r.get(1).unwrap_or(0.0),
            length_ft: r.get(2).unwrap_or(0), width_ft: r.get(3).unwrap_or(0),
            surface: r.get::<_,String>(4).unwrap_or_default(),
            latitude: r.get(5).unwrap_or(0.0), longitude: r.get(6).unwrap_or(0.0),
            elevation_ft: r.get(7).unwrap_or(0),
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut is_ = self.conn.prepare("SELECT i.Ident,i.Freq,i.LocCourse,i.GsAngle,i.Category,i.HasDme,r.Ident,i.Elevation FROM ILSes i JOIN Runways r ON i.RunwayID=r.ID WHERE r.AirportID=?1 ORDER BY i.Ident")?;
        ap.ils = is_.query_map(params![airport_id], |r| Ok(IlsRecord {
            ident: r.get::<_,String>(0).unwrap_or_default(), freq_mhz: decode_bcd_freq(r.get(1).unwrap_or(0)),
            loc_course: r.get(2).unwrap_or(0.0), gs_angle: r.get(3).unwrap_or(0.0),
            category: r.get::<_,String>(4).unwrap_or_default(),
            has_dme: r.get::<_,i32>(5).unwrap_or(0) != 0,
            runway_ident: r.get::<_,String>(6).unwrap_or_default(),
            elevation_ft: r.get(7).unwrap_or(0),
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut cs = self.conn.prepare("SELECT communication_type,communication_frequency,callsign,service_indicator FROM AirportCommunication WHERE airport_identifier=?1 OR icao_code=?1 ORDER BY communication_type,communication_frequency")?;
        ap.communications = cs.query_map(params![icao.to_uppercase()], |r| Ok(CommRecord {
            comm_type: r.get::<_,String>(0).unwrap_or_default(),
            frequency_mhz: r.get(1).unwrap_or(0.0),
            callsign: r.get::<_,String>(2).unwrap_or_default(),
            service_indicator: r.get::<_,String>(3).unwrap_or_default(),
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Some(ap))
    }

    pub fn fetch_waypoint_display_name(&self, ident: &str) -> rusqlite::Result<Option<String>> {
        match self.conn.query_row("SELECT Name FROM Waypoints WHERE Ident=?1 LIMIT 1", params![ident], |r| r.get::<_,String>(0)) {
            Ok(n) if !n.is_empty() && n != ident => Ok(Some(n)),
            _ => Ok(None),
        }
    }

    pub fn find_procedures_by_wpt(&self, wpt: &str, airport: &str, ptype: &str) -> rusqlite::Result<Vec<ProcedureInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT t.ID, t.FullName, t.Name, t.RwyID FROM TerminalLegs tl \
             JOIN Terminals t ON tl.TerminalID=t.ID JOIN Waypoints w ON tl.WptID=w.ID \
             WHERE w.Ident=?1 AND t.ICAO=?2 AND t.Proc=?3"
        )?;
        let rows: Vec<(i32,String,String,Option<i32>)> = stmt.query_map(
            params![wpt.to_uppercase(), airport.to_uppercase(), ptype],
            |r| Ok((r.get(0)?, r.get::<_,String>(1).unwrap_or_default(), r.get::<_,String>(2).unwrap_or_default(), r.get(3)?))
        )?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut results = Vec::new();
        for (tid, full, name, rwy_id) in rows {
            results.push(self.build_procedure(tid, &full, &name, rwy_id, ptype)?);
        }
        Ok(results)
    }

    pub fn fetch_all_procedures(&self, airport: &str, ptype: &str) -> rusqlite::Result<Vec<ProcedureInfo>> {
        let mut stmt = self.conn.prepare("SELECT ID, FullName, Name, RwyID FROM Terminals WHERE ICAO=?1 AND Proc=?2 ORDER BY FullName")?;
        let rows: Vec<(i32,String,String,Option<i32>)> = stmt.query_map(
            params![airport.to_uppercase(), ptype],
            |r| Ok((r.get(0)?, r.get::<_,String>(1).unwrap_or_default(), r.get::<_,String>(2).unwrap_or_default(), r.get(3)?))
        )?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut results = Vec::new();
        for (tid, full, name, rwy_id) in rows {
            results.push(self.build_procedure(tid, &full, &name, rwy_id, ptype)?);
        }
        Ok(results)
    }

    fn build_procedure(&self, tid: i32, full: &str, name: &str, rwy_id: Option<i32>, ptype: &str) -> rusqlite::Result<ProcedureInfo> {
        let mut info = ProcedureInfo {
            name: if name.is_empty() { full.into() } else { name.into() },
            full_name: full.into(),
            proc_type: if ptype == "1" { "SID".into() } else { "STAR".into() },
            ..Default::default()
        };

        let mut fs = self.conn.prepare("SELECT w.Ident FROM TerminalLegs tl LEFT JOIN Waypoints w ON tl.WptID=w.ID WHERE tl.TerminalID=?1 AND tl.WptID IS NOT NULL ORDER BY tl.ID")?;
        let fixes: Vec<String> = fs.query_map(params![tid], |r| Ok(r.get::<_,String>(0).unwrap_or_default()))?.filter_map(|s| s.ok().filter(|s| !s.is_empty())).collect();
        info.fixes = fixes;

        let mut ns = self.conn.prepare("SELECT DISTINCT n.Ident, nt.Desc, n.Freq FROM TerminalLegs tl JOIN Navaids n ON tl.NavID=n.ID JOIN NavaidTypes nt ON n.Type=nt.Type WHERE tl.TerminalID=?1 AND tl.NavID IS NOT NULL")?;
        info.nav_refs = ns.query_map(params![tid], |r| Ok(NavRef {
            ident: r.get::<_,String>(0).unwrap_or_default(),
            type_desc: r.get::<_,String>(1).unwrap_or_default(),
            freq_mhz: decode_bcd_freq(r.get(2).unwrap_or(0)),
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;

        if let Some(rid) = rwy_id {
            if let Ok(rwy) = self.conn.query_row("SELECT Ident,TrueHeading,Length,Width,Surface FROM Runways WHERE ID=?1", params![rid], |r| Ok((
                r.get::<_,String>(0).unwrap_or_default(), r.get::<_,f64>(1).unwrap_or(0.0),
                r.get::<_,i32>(2).unwrap_or(0), r.get::<_,i32>(3).unwrap_or(0),
                r.get::<_,String>(4).unwrap_or_default(),
            ))) {
                info.runway = rwy.0; info.runway_heading = rwy.1; info.runway_length_ft = rwy.2; info.runway_width_ft = rwy.3; info.runway_surface = rwy.4;
                if let Ok(ils) = self.conn.query_row("SELECT Ident,Freq,LocCourse,GsAngle,Category,HasDme FROM ILSes WHERE RunwayID=?1 LIMIT 1", params![rid], |r| Ok((
                    r.get::<_,String>(0).unwrap_or_default(), r.get::<_,i32>(1).unwrap_or(0),
                    r.get::<_,f64>(2).unwrap_or(0.0), r.get::<_,f64>(3).unwrap_or(0.0),
                    r.get::<_,String>(4).unwrap_or_default(), r.get::<_,i32>(5).unwrap_or(0) != 0,
                ))) {
                    info.ils_ident = ils.0; info.ils_freq_mhz = decode_bcd_freq(ils.1); info.ils_loc_course = ils.2; info.ils_gs_angle = ils.3; info.ils_category = ils.4; info.ils_has_dme = ils.5;
                }
            }
        }
        Ok(info)
    }
}
