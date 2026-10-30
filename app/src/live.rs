//! Live event data, fetched in the background on launch: the official Global news feed
//! (umamusume.com) and Moomoolator's event presets (umalator.app). Cached per user in
//! `%APPDATA%\umaLord\cache\events.json` (`~/.config/umaLord/cache` off Windows). For dates, conditions and disabled skills the official
//! announcement wins over Umalator's guess, which wins over the bundled
//! `knowledge/data/umalator.json`; Umalator gives the exact course (inner or outer layout).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use serde::{Deserialize, Serialize};

use crate::knowledge::{EventPreset, EventSource, Knowledge};

pub const NEWS_URL: &str = "https://umamusume.com/api/ajax/pr_info_index?format=json";
pub const UMALATOR_URL: &str = "https://umalator.app";
pub const UMA_GUIDE_URL: &str = "https://uma.guide";
/// The affinity tables change only when characters are added: refetch them weekly.
pub const AFFINITY_STALE_SECS: i64 = 7 * 86400;
/// An article on the official site.
pub fn article_url(id: u32) -> String {
    format!("https://umamusume.com/news/{id}/")
}
/// Refresh on launch when the cache is older than this.
pub const STALE_SECS: i64 = 6 * 3600;

const GROUNDS: [&str; 4] = ["Firm", "Good", "Soft", "Heavy"];
const WEATHERS: [&str; 4] = ["Sunny", "Cloudy", "Rainy", "Snowy"];
const SEASONS: [&str; 4] = ["Spring", "Summer", "Autumn", "Winter"];
const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    ChampionsMeeting,
    LegendRace,
    LeagueOfHeroes,
    #[default]
    Other,
}

/// A race's conditions as an announcement writes them, e.g.
/// "Chukyo / Turf / 1,200m (Sprint) / Left-Handed / Winter / Sunny / Firm".
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RaceConditions {
    /// The race's name, when given first (Legend Races: "Arima Kinen").
    #[serde(default)]
    pub race: Option<String>,
    pub track: String,
    pub surface: String,
    pub distance: u32,
    /// "inner", "outer" or empty.
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub direction: String,
    /// 1-based codes in the planner's chip order (Firm…, Sunny…, Spring…).
    pub ground: Option<u8>,
    pub weather: Option<u8>,
    pub season: Option<u8>,
    /// The line as written.
    pub text: String,
}

/// One news item, parsed.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Announcement {
    pub id: u32,
    pub title: String,
    /// When it was posted (UTC, seconds since 1970).
    pub posted: i64,
    pub kind: Kind,
    /// "Capricorn Cup", for Champions Meeting announcements.
    #[serde(default)]
    pub cup: Option<String>,
    #[serde(default)]
    pub league_selection: Option<i64>,
    #[serde(default)]
    pub round1: Option<i64>,
    /// The start of the event (the first time it names, for Legend Races and other events).
    #[serde(default)]
    pub starts: Option<i64>,
    #[serde(default)]
    pub ends: Option<i64>,
    /// Every from–to range in the text, e.g. a Legend Race's two availability periods.
    #[serde(default)]
    pub periods: Vec<(i64, i64)>,
    #[serde(default)]
    pub conditions: Option<RaceConditions>,
    /// Skills that won't activate, as written ("Night Races ◎").
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default)]
    pub image: Option<String>,
}

/// How fetching one source went.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SourceStatus {
    pub ok: bool,
    /// What went wrong, or what was found.
    pub message: String,
    /// When it last worked (UTC seconds).
    #[serde(default)]
    pub fetched_at: Option<i64>,
}

/// What the refreshes found; this is the cache file.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LiveData {
    /// The last refresh (UTC seconds), whatever came of it.
    pub checked_at: i64,
    pub news: SourceStatus,
    pub umalator: SourceStatus,
    pub announcements: Vec<Announcement>,
    /// Umalator's presets as fetched; course names are filled in when merging.
    pub umalator_presets: Vec<EventPreset>,
    /// uma.guide, for the affinity tables (fetched weekly; an empty message means it wasn't due).
    #[serde(default)]
    pub uma_guide: SourceStatus,
    #[serde(default)]
    pub affinity: Option<crate::affinity::AffinityTable>,
    /// The spark and win tables, from the same uma.guide download as the affinity tables.
    #[serde(default)]
    pub sparks: Option<Vec<crate::knowledge::SparkInfo>>,
    #[serde(default)]
    pub saddles: Option<std::collections::HashMap<u32, crate::knowledge::Saddle>>,
    /// The last try at the affinity tables (UTC seconds), so a failure waits a day to retry.
    #[serde(default)]
    pub affinity_tried: i64,
}

impl LiveData {
    /// Keep what a failed source had last time.
    pub fn keep_old(mut self, old: Option<&LiveData>) -> LiveData {
        if let Some(old) = old {
            if !self.news.ok {
                self.announcements = old.announcements.clone();
                self.news.fetched_at = old.news.fetched_at;
            }
            if !self.umalator.ok {
                self.umalator_presets = old.umalator_presets.clone();
                self.umalator.fetched_at = old.umalator.fetched_at;
            }
            // The affinity tables refresh on their own thread and write these themselves.
            self.affinity = old.affinity.clone();
            self.sparks = old.sparks.clone();
            self.saddles = old.saddles.clone();
            self.uma_guide = old.uma_guide.clone();
            self.affinity_tried = old.affinity_tried;
        }
        self
    }

    /// Whether the affinity tables are due a refetch: the newest copy (fetched, or the bundled one
    /// read on `bundled`) is over a week old, and the last try was over a day ago.
    pub fn affinity_due(d: Option<&LiveData>, bundled: &str) -> bool {
        let bundled = date_epoch(bundled.get(..10).unwrap_or_default()).unwrap_or(0);
        let newest = d.and_then(|d| d.uma_guide.fetched_at).unwrap_or(0).max(bundled);
        let tried = d.map_or(0, |d| d.affinity_tried);
        now() - newest > AFFINITY_STALE_SECS && now() - tried > 86_400
    }
}

// ---------------------------------------------------------------------------
// Time, without a date crate (UTC seconds since 1970)

/// Days since 1970-01-01 for a civil date (Howard Hinnant's days_from_civil).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month, day) for days since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn epoch(y: i64, m: u32, d: u32, h: u32, min: u32) -> i64 {
    days_from_civil(y, m, d) * 86_400 + i64::from(h) * 3600 + i64::from(min) * 60
}

/// "YYYY-MM-DD" (UTC) of a time.
pub fn utc_date(t: i64) -> String {
    let (y, m, d) = civil_from_days(t.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Seconds since 1970 for "YYYY-MM-DD" (midnight UTC).
pub fn date_epoch(date: &str) -> Option<i64> {
    let mut it = date.split('-');
    let (y, m, d) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    Some(epoch(y, m, d, 0, 0))
}

/// UTC seconds as `YYYY-MM-DDTHH:MM:SSZ`, which sorts by time.
pub fn iso(t: i64) -> String {
    let s = t.rem_euclid(86_400);
    format!("{}T{:02}:{:02}:{:02}Z", utc_date(t), s / 3600, s / 60 % 60, s % 60)
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A time in this computer's time zone (daylight saving for that date included):
/// (year, month, day, hour, minute, weekday 0 = Sunday). Falls back to UTC if the system can't say.
pub fn local(t: i64) -> (i64, u32, u32, u32, u32, u32) {
    let utc = || {
        let days = t.div_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        let s = t.rem_euclid(86_400);
        (y, m, d, (s / 3600) as u32, ((s / 60) % 60) as u32, (days + 4).rem_euclid(7) as u32)
    };
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            year: u16,
            month: u16,
            weekday: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            ms: u16,
        }
        unsafe extern "system" {
            fn SystemTimeToTzSpecificLocalTime(tz: *const std::ffi::c_void, utc: *const SystemTime, local: *mut SystemTime) -> i32;
        }
        let (y, m, d, h, mi, wd) = utc();
        let Ok(year) = u16::try_from(y) else { return utc() };
        let input = SystemTime { year, month: m as u16, weekday: wd as u16, day: d as u16, hour: h as u16, minute: mi as u16, second: 0, ms: 0 };
        let mut out = SystemTime::default();
        // SAFETY: both structs are SYSTEMTIME-shaped and live for the call; a null zone means the
        // computer's current time zone.
        let ok = unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &input, &mut out) };
        if ok != 0 {
            return (i64::from(out.year), u32::from(out.month), u32::from(out.day), u32::from(out.hour), u32::from(out.minute), u32::from(out.weekday));
        }
    }
    #[cfg(all(unix, target_pointer_width = "64"))]
    {
        /// POSIX `struct tm` as glibc, musl and macOS lay it out on 64-bit targets.
        #[repr(C)]
        struct Tm {
            sec: i32,
            min: i32,
            hour: i32,
            mday: i32,
            mon: i32,
            year: i32,
            wday: i32,
            yday: i32,
            isdst: i32,
            gmtoff: i64,
            zone: *const std::ffi::c_char,
        }
        unsafe extern "C" {
            fn tzset();
            fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
        }
        let mut out = Tm { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, gmtoff: 0, zone: std::ptr::null() };
        // SAFETY: `t` and `out` live for the call; tzset reads TZ (or /etc/localtime) first, which
        // localtime_r isn't required to do itself.
        let ok = unsafe {
            tzset();
            !localtime_r(&t, &mut out).is_null()
        };
        if ok {
            return (i64::from(out.year) + 1900, (out.mon + 1) as u32, out.mday as u32, out.hour as u32, out.min as u32, out.wday as u32);
        }
    }
    utc()
}

fn clock(h: u32, m: u32) -> String {
    let (h12, ampm) = match h {
        0 => (12, "a.m."),
        1..=11 => (h, "a.m."),
        12 => (12, "p.m."),
        _ => (h - 12, "p.m."),
    };
    if m == 0 { format!("{h12} {ampm}") } else { format!("{h12}:{m:02} {ampm}") }
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Fri Oct 31, 3 p.m." in local time.
pub fn fmt_local(t: i64) -> String {
    let (_, m, d, h, mi, wd) = local(t);
    format!("{} {} {d}, {}", WEEKDAYS[wd as usize % 7], MONTH_NAMES[(m as usize).clamp(1, 12) - 1], clock(h, mi))
}

/// "10 p.m. UTC, Oct 31".
pub fn fmt_utc(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let (_, m, d) = civil_from_days(days);
    let s = t.rem_euclid(86_400);
    format!("{} UTC, {} {d}", clock((s / 3600) as u32, ((s / 60) % 60) as u32), MONTH_NAMES[(m as usize).clamp(1, 12) - 1])
}

/// "Oct 31" from a "YYYY-MM-DD" date.
pub fn short_day(date: &str) -> String {
    let mut it = date.split('-').skip(1);
    match (it.next().and_then(|m| m.parse::<usize>().ok()), it.next().and_then(|d| d.parse::<u32>().ok())) {
        (Some(m), Some(d)) if (1..=12).contains(&m) => format!("{} {d}", MONTH_NAMES[m - 1]),
        _ => date.to_string(),
    }
}

/// "in 13 days", "in 5 h 20 min", "3 days ago".
pub fn fmt_until(secs: i64) -> String {
    let a = secs.abs();
    let text = if a >= 2 * 86_400 {
        format!("{} days", a / 86_400)
    } else if a >= 3600 {
        format!("{} h {} min", a / 3600, (a / 60) % 60)
    } else {
        format!("{} min", (a / 60).max(1))
    };
    if secs >= 0 { format!("in {text}") } else { format!("{text} ago") }
}

/// "3 min ago", "2 h ago", "4 days ago".
pub fn fmt_ago(secs: i64) -> String {
    let a = secs.max(0);
    if a < 90 {
        "just now".into()
    } else if a < 3600 {
        format!("{} min ago", a / 60)
    } else if a < 2 * 86_400 {
        format!("{} h ago", a / 3600)
    } else {
        format!("{} days ago", a / 86_400)
    }
}

// ---------------------------------------------------------------------------
// Parsing announcements

/// HTML as plain lines: `<br>` and block ends become line breaks, headings become "## " lines,
/// tags go, entities and no-break spaces are decoded.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    // Line breaks in the HTML source are just spaces; only tags break lines.
    let html = html.replace(['\r', '\n'], " ");
    let mut rest = html.as_str();
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            rest = &rest[i..];
            break;
        };
        let tag = rest[i + 1..i + j].trim().to_ascii_lowercase();
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        match name.as_str() {
            "br" => out.push('\n'),
            "h1" | "h2" | "h3" | "h4" if !tag.starts_with('/') => out.push_str("\n## "),
            "h1" | "h2" | "h3" | "h4" | "p" | "div" | "li" | "tr" | "ul" | "ol" => out.push('\n'),
            _ => {}
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    let text = decode_entities(&out).replace(['\u{a0}', '\u{2007}', '\u{202f}'], " ");
    // Trim each line, tidy headings, and keep at most one blank line in a row.
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines().map(str::trim) {
        let l = match l.strip_prefix("##") {
            Some(h) => format!("## {}", h.trim()),
            None => l.to_string(),
        };
        if l.is_empty() && lines.last().is_none_or(|p| p.is_empty()) {
            continue;
        }
        lines.push(l);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').filter(|e| *e <= 10);
        let decoded = end.and_then(|e| {
            let name = &tail[1..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                "ndash" => Some('–'),
                "mdash" => Some('—'),
                _ if name.starts_with("#x") || name.starts_with("#X") => u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32),
                _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            }?;
            Some((c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A moment written like "10:00 p.m., Oct 27, 2026"; the year may be left for later.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Moment {
    year: Option<i64>,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    /// Byte span in the line.
    start: usize,
    end: usize,
}

impl Moment {
    fn epoch(&self, year: i64) -> i64 {
        epoch(year, self.month, self.day, self.hour, self.minute)
    }
}

/// Every moment in a line, in order.
fn moments(line: &str) -> Vec<Moment> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit()) {
            if let Some(m) = moment_at(line, i) {
                i = m.end;
                out.push(m);
                continue;
            }
        }
        i += 1;
    }
    out
}

/// "H:MM a.m., Mon D[, YYYY]" starting at byte `i`.
fn moment_at(line: &str, i: usize) -> Option<Moment> {
    let s = &line[i..];
    let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
    let hn = digits(s);
    if !(1..=2).contains(&hn) || s.as_bytes().get(hn) != Some(&b':') {
        return None;
    }
    let hour: u32 = s[..hn].parse().ok()?;
    let s2 = &s[hn + 1..];
    if digits(s2) != 2 {
        return None;
    }
    let minute: u32 = s2[..2].parse().ok()?;
    let s3 = s2[2..].trim_start();
    let (pm, s4) = if let Some(r) = s3.strip_prefix("p.m.") {
        (true, r)
    } else if let Some(r) = s3.strip_prefix("a.m.") {
        (false, r)
    } else {
        return None;
    };
    let s5 = s4.strip_prefix(',').unwrap_or(s4).trim_start();
    let word: String = s5.chars().take_while(char::is_ascii_alphabetic).collect();
    let month = MONTHS.iter().position(|m| word.len() >= 3 && word[..3].eq_ignore_ascii_case(m))? as u32 + 1;
    let s6 = s5[word.len()..].trim_start_matches('.').trim_start();
    let dn = digits(s6);
    if !(1..=2).contains(&dn) {
        return None;
    }
    let day: u32 = s6[..dn].parse().ok()?;
    let mut rest = &s6[dn..];
    let mut year = None;
    if let Some(r) = rest.strip_prefix(", ").or_else(|| rest.strip_prefix(','))
        && digits(r.trim_start()) == 4
    {
        let r = r.trim_start();
        year = r[..4].parse().ok();
        rest = &r[4..];
    }
    let hour = match (hour, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    };
    let end = line.len() - rest.len();
    Some(Moment { year, month, day, hour, minute, start: i, end })
}

/// The times in a line as UTC seconds, with ranges ("A–B") paired. A moment without a year
/// takes the next one's (or `fallback`); a range that would run backwards starts a year earlier.
fn times(line: &str, fallback: i64) -> (Vec<i64>, Vec<(i64, i64)>) {
    let ms = moments(line);
    let years: Vec<i64> = (0..ms.len()).map(|k| ms[k..].iter().find_map(|m| m.year).unwrap_or(fallback)).collect();
    let mut singles = Vec::new();
    let mut ranges = Vec::new();
    let mut k = 0;
    while k < ms.len() {
        let joined = ms.get(k + 1).is_some_and(|n| {
            let between = line[ms[k].end..n.start].trim();
            matches!(between, "–" | "-" | "—" | "to" | "~")
        });
        if joined {
            let end = ms[k + 1].epoch(years[k + 1]);
            let mut start = ms[k].epoch(years[k]);
            if start > end && ms[k].year.is_none() {
                start = ms[k].epoch(years[k] - 1);
            }
            ranges.push((start, end));
            k += 2;
        } else {
            singles.push(ms[k].epoch(years[k]));
            k += 1;
        }
    }
    (singles, ranges)
}

/// "Chukyo / Turf / 1,200m (Sprint) / Left-Handed / Winter / Sunny / Firm".
pub fn parse_conditions(text: &str) -> Option<RaceConditions> {
    let parts: Vec<&str> = text.split('/').map(str::trim).filter(|p| !p.is_empty()).collect();
    let si = parts.iter().position(|p| p.eq_ignore_ascii_case("Turf") || p.eq_ignore_ascii_case("Dirt"))?;
    if si == 0 {
        return None;
    }
    let track = parts[si - 1].to_string();
    let race = (si >= 2).then(|| parts[..si - 1].join(" / "));
    let mut c = RaceConditions { race, track, surface: parts[si].to_string(), text: parts.join(" / "), ..Default::default() };
    for p in &parts[si + 1..] {
        let lower = p.to_ascii_lowercase();
        let code = |names: &[&str; 4]| names.iter().position(|n| n.eq_ignore_ascii_case(p)).map(|i| i as u8 + 1);
        if let Some(m) = lower.find('m').filter(|_| p.chars().next().is_some_and(|ch| ch.is_ascii_digit())) {
            c.distance = lower[..m].replace(',', "").trim().parse().unwrap_or(0);
        } else if lower == "inner" || lower == "outer" {
            c.layout = lower;
        } else if lower.ends_with("-handed") {
            c.direction = p.split('-').next().unwrap_or("").to_string();
        } else if let Some(g) = code(&GROUNDS) {
            c.ground = Some(g);
        } else if let Some(w) = code(&WEATHERS) {
            c.weather = Some(w);
        } else if let Some(s) = code(&SEASONS) {
            c.season = Some(s);
        }
    }
    (c.distance > 0).then_some(c)
}

/// "Champions Meeting: Capricorn Cup is coming…" → "Capricorn Cup".
fn cup_name(title: &str) -> Option<String> {
    let after = &title[title.find("Champions Meeting:")? + "Champions Meeting:".len()..];
    let after = after.trim_start();
    let name = match after.find(" Cup") {
        Some(i) => after[..i + 4].to_string(),
        None => after.split([' ', '!']).take_while(|w| !matches!(*w, "is" | "has" | "will")).collect::<Vec<_>>().join(" "),
    };
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Parse one news item.
pub fn parse_announcement(id: u32, title: &str, message: &str, posted: i64) -> Announcement {
    let kind = if title.contains("Champions Meeting") {
        Kind::ChampionsMeeting
    } else if title.contains("Legend Race") {
        Kind::LegendRace
    } else if title.contains("League of Heroes") {
        Kind::LeagueOfHeroes
    } else {
        Kind::Other
    };
    let mut a = Announcement { id, title: title.to_string(), posted, kind, cup: cup_name(title), ..Default::default() };
    let year = civil_from_days(posted.div_euclid(86_400)).0;
    let text = html_to_text(message);
    let lines: Vec<&str> = text.lines().collect();
    let mut heading = String::new();
    let mut conditions: Option<String> = None;
    let mut in_conditions = false;
    let mut in_disabled = false;
    for line in &lines {
        if let Some(h) = line.strip_prefix("## ") {
            heading = h.trim().to_ascii_lowercase();
            in_conditions = heading.starts_with("race condition");
            in_disabled = false;
            continue;
        }
        // A blank line ends a section, once it has something in it.
        if line.is_empty() {
            in_conditions &= conditions.as_deref().is_none_or(|c| c.trim().is_empty());
            in_disabled &= a.disabled.is_empty();
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("the following skills will not activate") {
            in_conditions = false;
            in_disabled = true;
            continue;
        }
        if in_disabled {
            match line.strip_prefix('•').or_else(|| line.strip_prefix('・')).or_else(|| line.strip_prefix("- ")) {
                Some(name) => a.disabled.push(name.split_whitespace().collect::<Vec<_>>().join(" ")),
                None => in_disabled = false,
            }
            continue;
        }
        if in_conditions {
            let c = conditions.get_or_insert_with(String::new);
            if !c.is_empty() && !c.trim_end().ends_with('/') {
                c.push_str(" / ");
            }
            c.push(' ');
            c.push_str(line);
            continue;
        }
        // Bullets are rules ("after 10 p.m., Oct 10 you can't…"), not the schedule.
        if line.starts_with('•') {
            continue;
        }
        let (singles, ranges) = times(line, year);
        a.periods.extend(ranges.iter().copied());
        let first = ranges.first().map(|r| r.0).or(singles.first().copied());
        if lower.starts_with("league selection:") {
            a.league_selection = a.league_selection.or(first);
        } else if lower.starts_with("round 1:") {
            a.round1 = a.round1.or(first);
        } else if heading.starts_with("league selection") {
            a.league_selection = a.league_selection.or(first);
        } else if heading == "round 1" {
            a.round1 = a.round1.or(first);
        } else if heading.contains("availability period") {
            if let Some((s, e)) = ranges.first() {
                a.starts = a.starts.or(Some(*s));
                a.ends = a.ends.max(Some(*e));
            }
        } else if a.starts.is_none() && heading.is_empty() && (lower.contains("set to begin") || lower.contains("has begun") || lower.contains("as of")) {
            a.starts = first;
        }
    }
    if let Some(c) = conditions {
        a.conditions = parse_conditions(&c.replace("/  /", "/").replace(" /  ", " / "));
    }
    if a.kind == Kind::ChampionsMeeting {
        a.round1 = a.round1.or(a.starts);
        a.starts = a.starts.or(a.league_selection).or(a.round1);
    }
    a
}

#[derive(Deserialize)]
struct NewsResponse {
    response_code: i64,
    #[serde(default)]
    information_list: Vec<NewsItem>,
}

#[derive(Deserialize)]
struct NewsItem {
    announce_id: u32,
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    post_at: String,
    #[serde(default)]
    image: Option<String>,
}

/// "2026-10-17 22:00:00" (UTC) as seconds.
fn parse_post_at(s: &str) -> i64 {
    let (date, time) = s.split_once(' ').unwrap_or((s, "0:0:0"));
    let mut t = time.split(':').map(|x| x.parse::<u32>().unwrap_or(0));
    date_epoch(date).unwrap_or(0) + i64::from(t.next().unwrap_or(0)) * 3600 + i64::from(t.next().unwrap_or(0)) * 60
}

/// The news API's JSON as announcements.
pub fn parse_news(json: &str) -> Result<Vec<Announcement>, String> {
    let r: NewsResponse = serde_json::from_str(json).map_err(|e| format!("unexpected reply: {e}"))?;
    if r.response_code != 1 {
        return Err(format!("the site answered with code {}", r.response_code));
    }
    Ok(r.information_list
        .into_iter()
        .map(|it| {
            let mut a = parse_announcement(it.announce_id, &it.title, &it.message, parse_post_at(&it.post_at));
            a.image = it.image.filter(|i| !i.is_empty());
            a
        })
        .collect())
}

/// Moomoolator's presets from its app bundle: `{id:21,type:0,name:"Capricorn Cup",date:"…",
/// courseId:10701,season:4,ground:1,weather:1,time:2,confirmed:!1}` (`!1` is false).
pub fn parse_umalator_presets(bundle: &str) -> Vec<EventPreset> {
    let mut out = Vec::new();
    let mut rest = bundle;
    while let Some(i) = rest.find("{id:") {
        let tail = &rest[i + 1..];
        let Some(end) = tail.find('}') else { break };
        let body = &tail[..end];
        rest = &tail[end..];
        let mut fields = std::collections::HashMap::new();
        for kv in body.split(',') {
            if let Some((k, v)) = kv.split_once(':') {
                fields.insert(k.trim(), v.trim().trim_matches('"'));
            }
        }
        let num = |k: &str| fields.get(k).and_then(|v| v.parse::<u32>().ok());
        let (Some(id), Some(kind), Some(name), Some(date), Some(course), Some(season), Some(ground), Some(weather), Some(time), Some(confirmed)) = (
            num("id"),
            num("type"),
            fields.get("name"),
            fields.get("date"),
            num("courseId"),
            num("season"),
            num("ground"),
            num("weather"),
            num("time"),
            fields.get("confirmed"),
        ) else {
            continue;
        };
        out.push(EventPreset {
            id,
            event: if kind == 1 { "League of Heroes".into() } else { "Champions Meeting".into() },
            name: name.to_string(),
            date: date.to_string(),
            course,
            course_name: String::new(),
            ground: ground as u8,
            weather: weather as u8,
            season: season as u8,
            time: time as u8,
            conditions: String::new(),
            confirmed: *confirmed == "!0",
            source: EventSource::Umalator,
            ..Default::default()
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Merging

/// "Firm · Sunny · Winter · Midday".
pub fn conditions_text(p: &EventPreset) -> String {
    fn named(names: &[&'static str; 4], code: u8) -> Option<&'static str> {
        code.checked_sub(1).and_then(|i| names.get(usize::from(i))).copied()
    }
    let time = match p.time {
        2 => Some("Midday"),
        3 => Some("Evening"),
        4 => Some("Night"),
        _ => None,
    };
    [named(&GROUNDS, p.ground), named(&WEATHERS, p.weather), named(&SEASONS, p.season), time].into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

fn course_name(kb: &Knowledge, course: u32) -> String {
    kb.umalator_courses
        .iter()
        .find(|c| c.id == course)
        .map(|c| format!("{} {} {}m{}", kb.track_name(c.track_id), c.surface, c.distance, if c.layout.is_empty() { String::new() } else { format!(" ({})", c.layout) }))
        .unwrap_or_default()
}

/// Whether an Umalator course fits an announcement's conditions.
fn course_fits(kb: &Knowledge, course: u32, c: &RaceConditions) -> bool {
    kb.umalator_courses.iter().find(|u| u.id == course).is_some_and(|u| {
        kb.track_name(u.track_id).eq_ignore_ascii_case(&c.track)
            && u.surface.eq_ignore_ascii_case(&c.surface)
            && u.distance == c.distance
            && (c.layout.is_empty() || u.layout.starts_with(&c.layout))
    })
}

/// The Umalator course for an announcement's conditions: the matching layout, else one an event
/// already uses, else the first.
fn find_course(kb: &Knowledge, c: &RaceConditions, used: &[u32]) -> Option<u32> {
    let fits: Vec<u32> = kb.umalator_courses.iter().filter(|u| course_fits(kb, u.id, c)).map(|u| u.id).collect();
    fits.iter().find(|id| used.contains(id)).or(fits.first()).copied()
}

/// Skill ids for names as announcements write them ("Night Races ◎"). Unmatched names are
/// returned too, to log.
pub fn resolve_skills(kb: &Knowledge, names: &[String]) -> (Vec<u32>, Vec<String>) {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase().replace(" ◎", "◎").replace(" ○", "○").replace(" ×", "×");
    let mut ids = Vec::new();
    let mut missing = Vec::new();
    for n in names {
        let want = norm(n);
        let mut found: Vec<u32> = kb.skills.values().filter(|s| norm(&s.name) == want).map(|s| s.id).collect();
        found.sort_unstable();
        match found.first() {
            Some(id) => ids.push(*id),
            None => missing.push(n.clone()),
        }
    }
    (ids, missing)
}

/// The event list the app uses: Umalator's live presets (else the bundled ones), then each
/// official Champions Meeting announcement on top, matched by cup name within 45 days.
pub fn merge(kb: &Knowledge, bundled: &[EventPreset], live: &LiveData) -> Vec<EventPreset> {
    let mut out: Vec<EventPreset> = if live.umalator_presets.is_empty() {
        bundled.iter().cloned().map(|mut p| {
            p.source = EventSource::Bundled;
            p
        }).collect()
    } else {
        let mut v: Vec<EventPreset> = live
            .umalator_presets
            .iter()
            .cloned()
            .map(|mut p| {
                p.course_name = course_name(kb, p.course);
                p.conditions = conditions_text(&p);
                p.source = EventSource::Umalator;
                p
            })
            .collect();
        // Older events the live list no longer has stay.
        for b in bundled {
            if !v.iter().any(|p| p.event == b.event && p.id == b.id) {
                let mut b = b.clone();
                b.source = EventSource::Bundled;
                v.push(b);
            }
        }
        v
    };
    let mut cms: Vec<&Announcement> = live.announcements.iter().filter(|a| a.kind == Kind::ChampionsMeeting && a.cup.is_some()).collect();
    // Later announcements refine earlier ones.
    cms.sort_by_key(|a| a.posted);
    for a in cms {
        let cup = a.cup.as_deref().unwrap_or_default();
        let at = a.round1.or(a.league_selection).or(a.starts).unwrap_or(a.posted);
        let idx = out
            .iter()
            .position(|p| p.is_cm() && p.name.eq_ignore_ascii_case(cup) && date_epoch(&p.date).is_some_and(|d| (d - at).abs() <= 45 * 86_400));
        let i = match idx {
            Some(i) => i,
            None => {
                out.push(EventPreset { event: "Champions Meeting".into(), name: cup.to_string(), date: utc_date(at), ..Default::default() });
                out.len() - 1
            }
        };
        let used: Vec<u32> = out.iter().map(|p| p.course).collect();
        let p = &mut out[i];
        p.league_selection = a.league_selection.or(p.league_selection);
        p.round1 = a.round1.or(p.round1);
        p.ends = a.ends.or(p.ends);
        if let Some(r1) = p.round1 {
            p.date = utc_date(r1);
        }
        if let Some(c) = &a.conditions {
            if !course_fits(kb, p.course, c)
                && let Some(course) = find_course(kb, c, &used)
            {
                p.course = course;
            }
            p.course_name = course_name(kb, p.course);
            p.ground = c.ground.unwrap_or(p.ground);
            p.weather = c.weather.unwrap_or(p.weather);
            p.season = c.season.unwrap_or(p.season);
            if p.time == 0 {
                p.time = 2;
            }
            p.conditions = conditions_text(p);
            p.official_conditions = Some(c.text.clone());
        }
        if !a.disabled.is_empty() {
            let (ids, missing) = resolve_skills(kb, &a.disabled);
            if !missing.is_empty() {
                eprintln!("{}: no skill named {}", cup, missing.join(", "));
            }
            p.disabled_skills = ids;
        }
        p.announced = true;
        p.source = EventSource::Official;
        p.article = Some(a.id);
    }
    out.sort_by(|a, b| a.date.cmp(&b.date));
    out
}

// ---------------------------------------------------------------------------
// Fetching and the cache

/// GET (or POST `body` as JSON) with curl (Windows ships curl.exe): no TLS crates needed.
fn fetch(url: &str, body: Option<&str>) -> Result<String, String> {
    fetch_within(url, body, 15)
}

fn fetch_within(url: &str, body: Option<&str>, secs: u32) -> Result<String, String> {
    let mut cmd = std::process::Command::new(crate::ask::CURL);
    cmd.args(["-sSfL", "--max-time", &secs.to_string(), "-A", &format!("umaLord/{}", env!("CARGO_PKG_VERSION"))]);
    if let Some(b) = body {
        cmd.args(["-H", "Content-Type: application/json", "--data-raw", b]);
    }
    cmd.arg(url);
    let out = crate::ask::hidden(&mut cmd).output().map_err(|e| format!("couldn't run {}: {e}", crate::ask::CURL))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "no answer".into() } else { err.trim_start_matches("curl: ").to_string() });
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

/// The news feed, newest first; pages back (up to 4 pages of 50) until it holds a Champions Meeting.
fn fetch_news() -> Result<Vec<Announcement>, String> {
    let mut all = Vec::new();
    for page in 0..4 {
        let body = format!(r#"{{"announce_label":0,"limit":50,"offset":{}}}"#, page * 50);
        let items = parse_news(&fetch(NEWS_URL, Some(&body))?)?;
        let done = items.len() < 50 || items.iter().any(|a| a.kind == Kind::ChampionsMeeting);
        all.extend(items);
        if done {
            break;
        }
    }
    Ok(all)
}

/// Moomoolator's presets from its current app bundle.
fn fetch_umalator() -> Result<Vec<EventPreset>, String> {
    let page = fetch(&format!("{UMALATOR_URL}/"), None)?;
    let start = page.find("/assets/index-").ok_or("couldn't find the app bundle")?;
    let path: String = page[start..].chars().take_while(|c| !matches!(c, '"' | '\'' | ' ' | '>')).collect();
    let presets = parse_umalator_presets(&fetch(&format!("{UMALATOR_URL}{path}"), None)?);
    if presets.is_empty() { Err("no event presets in the app bundle".into()) } else { Ok(presets) }
}

/// The affinity tables from uma.guide's data chunk (about 10 MB), found the way
/// `tools/update_knowledge.py` finds it: the characters page, then its app bundle.
/// uma.guide's game tables the app refreshes itself: affinity, sparks and win records, all read
/// from its data chunk. The spark and win tables are optional, so a change there never costs the
/// affinity tables.
pub struct UmaGuideTables {
    pub affinity: crate::affinity::AffinityTable,
    pub sparks: Option<Vec<crate::knowledge::SparkInfo>>,
    pub saddles: Option<std::collections::HashMap<u32, crate::knowledge::Saddle>>,
}

pub fn fetch_uma_guide_tables() -> Result<UmaGuideTables, String> {
    let page = fetch(&format!("{UMA_GUIDE_URL}/characters/"), None)?;
    let app = find_asset(&page, "/assets/app.").ok_or("couldn't find uma.guide's app bundle")?;
    let bundle = fetch(&format!("{UMA_GUIDE_URL}{app}"), None)?;
    let chunk = find_asset(&format!("{page} {bundle}"), "chunks/uma-data.").ok_or("couldn't find uma.guide's data chunk")?;
    let url = if chunk.starts_with('/') { format!("{UMA_GUIDE_URL}{chunk}") } else { format!("{UMA_GUIDE_URL}/assets/{chunk}") };
    let data = fetch_within(&url, None, 90)?;
    Ok(UmaGuideTables {
        affinity: crate::affinity::AffinityTable::from_uma_guide(&data, &iso(now()))?,
        sparks: crate::veterans::sparks_from_uma_guide(&data).ok(),
        saddles: crate::veterans::saddles_from_uma_guide(&data).ok(),
    })
}

/// The first `prefix….js` path in `text`.
fn find_asset(text: &str, prefix: &str) -> Option<String> {
    let mut rest = text;
    while let Some(i) = rest.find(prefix) {
        let path: String = rest[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || "/._-".contains(*c)).collect();
        if path.ends_with(".js") {
            return Some(path);
        }
        rest = &rest[i + prefix.len()..];
    }
    None
}

/// Both sources, as one `LiveData` (failed sources marked, with what went wrong).
pub fn refresh() -> LiveData {
    let t = now();
    let mut d = LiveData { checked_at: t, ..Default::default() };
    match fetch_news() {
        Ok(a) => {
            let cms = a.iter().filter(|x| x.kind == Kind::ChampionsMeeting).count();
            d.news = SourceStatus { ok: true, message: format!("{} news items, {cms} about Champions Meetings", a.len()), fetched_at: Some(t) };
            d.announcements = a;
        }
        Err(e) => d.news = SourceStatus { ok: false, message: e, fetched_at: None },
    }
    match fetch_umalator() {
        Ok(p) => {
            d.umalator = SourceStatus { ok: true, message: format!("{} event presets", p.len()), fetched_at: Some(t) };
            d.umalator_presets = p;
        }
        Err(e) => d.umalator = SourceStatus { ok: false, message: e, fetched_at: None },
    }
    d
}

/// Refresh on a background thread; the receiver gets the result once.
pub fn spawn_refresh(ctx: Option<eframe::egui::Context>) -> Receiver<LiveData> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(refresh());
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    });
    rx
}

/// The affinity tables on their own thread: a 10 MB download mustn't hold up the schedule.
pub fn spawn_affinity(ctx: eframe::egui::Context) -> Receiver<Result<UmaGuideTables, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(fetch_uma_guide_tables());
        ctx.request_repaint();
    });
    rx
}

pub fn cache_path() -> Option<PathBuf> {
    crate::settings::path().and_then(|p| p.parent().map(|d| d.join("cache").join("events.json")))
}

pub fn load_cache() -> Option<LiveData> {
    serde_json::from_str(&std::fs::read_to_string(cache_path()?).ok()?).ok()
}

pub fn save_cache(d: &LiveData) {
    if let Some(p) = cache_path()
        && p.parent().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
        && let Ok(text) = serde_json::to_string(d)
    {
        let _ = std::fs::write(p, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
    }

    fn news() -> Vec<Announcement> {
        parse_news(&fixture("news_sample.json")).unwrap()
    }

    fn at(y: i64, m: u32, d: u32, h: u32, min: u32) -> i64 {
        epoch(y, m, d, h, min)
    }

    #[test]
    fn dates_round_trip() {
        for days in [-1000, 0, 20_000, 20_744] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(utc_date(at(2026, 10, 31, 22, 0)), "2026-10-31");
        assert_eq!(date_epoch("2026-10-31"), Some(at(2026, 10, 31, 0, 0)));
    }

    #[test]
    fn reads_times_and_ranges() {
        let (s, r) = times("League Selection: From 10:00 p.m., Oct 27, 2026 (UTC)", 2026);
        assert_eq!((s, r), (vec![at(2026, 10, 27, 22, 0)], vec![]));
        let (_, r) = times("10:00 p.m., Oct 10–9:59 p.m., Oct 16, 2026 (UTC)", 2020);
        assert_eq!(r, vec![(at(2026, 10, 10, 22, 0), at(2026, 10, 16, 21, 59))]);
        let (_, r) = times("10:00 a.m., Oct 15–9:59 p.m., Oct 15, 2026 (UTC)", 2026);
        assert_eq!(r, vec![(at(2026, 10, 15, 10, 0), at(2026, 10, 15, 21, 59))]);
        // A range over New Year with the year only at the end.
        let (_, r) = times("10:00 p.m., Dec 28–9:59 p.m., Jan 3, 2027 (UTC)", 2026);
        assert_eq!(r, vec![(at(2026, 12, 28, 22, 0), at(2027, 1, 3, 21, 59))]);
        let (s, _) = times("12:00 a.m., Nov 1, 2026 and 12:30 p.m., Nov 2, 2026", 2026);
        assert_eq!(s, vec![at(2026, 11, 1, 0, 0), at(2026, 11, 2, 12, 30)]);
    }

    #[test]
    fn strips_html() {
        let t = html_to_text("\u{a0}A&amp;B<br>\n<h2 class=\"heading\">\u{a0}Race Conditions</h2>\n\u{a0}Chukyo / <br>Firm&#39;s &#x2013; ok<br><br>Next");
        assert_eq!(t, "A&B\n\n## Race Conditions\nChukyo /\nFirm's – ok\n\nNext");
    }

    /// "…is coming": league selection and Round 1 times, conditions across a line break, and the
    /// nine disabled skills.
    #[test]
    fn parses_a_coming_soon_announcement() {
        let a = news().into_iter().find(|a| a.id == 1100).unwrap();
        assert_eq!(a.kind, Kind::ChampionsMeeting);
        assert_eq!(a.cup.as_deref(), Some("Capricorn Cup"));
        assert_eq!(a.league_selection, Some(at(2026, 10, 27, 22, 0)));
        assert_eq!(a.round1, Some(at(2026, 10, 31, 22, 0)));
        assert_eq!(a.ends, None, "not announced yet");
        let c = a.conditions.unwrap();
        assert_eq!((c.track.as_str(), c.surface.as_str(), c.distance, c.layout.as_str(), c.direction.as_str()), ("Chukyo", "Turf", 1200, "", "Left"));
        assert_eq!((c.season, c.weather, c.ground), (Some(4), Some(1), Some(1)), "Winter, Sunny, Firm");
        assert_eq!(a.disabled.len(), 9);
        assert_eq!(a.disabled[0], "Night Races ◎");
        assert_eq!(a.disabled[8], "Collaborative Graded Races ×");
    }

    /// "…has begun" and "…is here": the full schedule.
    #[test]
    fn parses_the_full_schedule() {
        for id in [1086, 1087] {
            let a = news().into_iter().find(|a| a.id == id).unwrap();
            assert_eq!(a.cup.as_deref(), Some("Sagittarius Cup"), "{id}");
            assert_eq!(a.league_selection, Some(at(2026, 10, 7, 22, 0)), "{id}");
            assert_eq!(a.round1, Some(at(2026, 10, 10, 22, 0)), "{id}");
            assert_eq!(a.ends, Some(at(2026, 10, 16, 21, 59)), "{id}");
            let c = a.conditions.unwrap();
            assert_eq!((c.track.as_str(), c.distance, c.layout.as_str()), ("Nakayama", 2500, "inner"));
            assert_eq!((c.season, c.weather, c.ground), (Some(4), Some(2), Some(2)), "Winter, Cloudy, Good");
            assert_eq!(a.disabled.len(), 9, "{id}");
        }
    }

    #[test]
    fn parses_a_legend_race() {
        let a = news().into_iter().find(|a| a.id == 1112).unwrap();
        assert_eq!(a.kind, Kind::LegendRace);
        assert_eq!(a.starts, Some(at(2026, 10, 19, 22, 0)));
        assert_eq!(a.periods, vec![(at(2026, 10, 19, 22, 0), at(2026, 10, 22, 14, 59)), (at(2026, 10, 22, 15, 0), at(2026, 10, 25, 14, 59))]);
        let c = a.conditions.unwrap();
        assert_eq!((c.race.as_deref(), c.track.as_str(), c.distance, c.layout.as_str()), (Some("Arima Kinen"), "Nakayama", 2500, "inner"));
        let story = news().into_iter().find(|a| a.id == 1125).unwrap();
        assert_eq!(story.kind, Kind::Other);
        assert_eq!(story.periods, vec![(at(2026, 10, 20, 22, 0), at(2026, 11, 1, 21, 59))]);
    }

    #[test]
    fn reads_umalator_presets() {
        let p = parse_umalator_presets(&fixture("umalator_presets_sample.js"));
        let cap = p.iter().find(|e| e.name == "Capricorn Cup").unwrap();
        assert_eq!((cap.id, cap.course, cap.date.as_str(), cap.confirmed), (21, 10701, "2026-10-25", false));
        assert!(p.iter().all(|e| e.is_cm()));
    }

    fn kb() -> Knowledge {
        Knowledge::load(Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()))
    }

    /// All nine disabled-skill names resolve to skill ids.
    #[test]
    fn resolves_disabled_skills() {
        let kb = kb();
        let a = news().into_iter().find(|a| a.id == 1100).unwrap();
        let (ids, missing) = resolve_skills(&kb, &a.disabled);
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(ids, vec![202231, 202232, 202233, 202241, 202242, 202243, 202251, 202252, 202253]);
    }

    /// The official announcement beats Umalator's guess; Umalator's exact course stays when it
    /// fits; and with no Umalator data the bundled presets are the base.
    #[test]
    fn official_dates_win_and_courses_stay_exact() {
        let kb = kb();
        let bundled = kb.umalator_presets.clone();
        let live = LiveData { announcements: news(), umalator_presets: parse_umalator_presets(&fixture("umalator_presets_sample.js")), ..Default::default() };
        let merged = merge(&kb, &bundled, &live);
        let cap = merged.iter().find(|e| e.name == "Capricorn Cup" && e.date.as_str() > "2026-09").unwrap();
        assert_eq!(cap.date, "2026-10-31", "Round 1, not Umalator's 2026-10-25");
        assert_eq!((cap.league_selection, cap.round1), (Some(at(2026, 10, 27, 22, 0)), Some(at(2026, 10, 31, 22, 0))));
        assert!(cap.announced && cap.source == EventSource::Official);
        assert_eq!(cap.course, 10701, "Chukyo Turf 1200m");
        assert_eq!(cap.disabled_skills.len(), 9);
        assert_eq!(cap.conditions, "Firm · Sunny · Winter · Midday");
        let sag = merged.iter().find(|e| e.name == "Sagittarius Cup" && e.date.as_str() > "2026-09").unwrap();
        assert_eq!((sag.date.as_str(), sag.course, sag.ends), ("2026-10-10", 10506, Some(at(2026, 10, 16, 21, 59))));
        let aqu = merged.iter().find(|e| e.name == "Aquarius Cup" && e.date.as_str() > "2026-09").unwrap();
        assert!(!aqu.announced && aqu.source == EventSource::Umalator, "not announced: Umalator's estimate");
        assert!(merged.windows(2).all(|w| w[0].date <= w[1].date));
        // Offline, without Umalator: the bundled presets, still corrected by the official news.
        let offline = merge(&kb, &bundled, &LiveData { announcements: news(), ..Default::default() });
        let cap = offline.iter().find(|e| e.name == "Capricorn Cup" && e.date.as_str() > "2026-09").unwrap();
        assert_eq!((cap.date.as_str(), cap.course), ("2026-10-31", 10701));
        assert!(offline.iter().filter(|e| !e.announced).all(|e| e.source == EventSource::Bundled));
    }

    /// An announcement with an inner/outer layout picks that layout when Umalator guessed another
    /// course, and one for a cup Umalator doesn't have still appears.
    #[test]
    fn maps_courses_for_new_and_changed_cups() {
        let kb = kb();
        let mut a = news().into_iter().find(|a| a.id == 1087).unwrap();
        a.cup = Some("Ophiuchus Cup".into());
        let live = LiveData { announcements: vec![a], ..Default::default() };
        let merged = merge(&kb, &[], &live);
        let oph = merged.iter().find(|e| e.name == "Ophiuchus Cup").unwrap();
        assert_eq!((oph.course, oph.date.as_str(), oph.event.as_str()), (10506, "2026-10-10", "Champions Meeting"), "Nakayama Turf 2500m (inner)");
    }

    /// Hits the live feed, Umalator and uma.guide: `cargo test --release live_sources -- --ignored --nocapture`.
    /// The fetched affinity tables must equal the bundled ones unless a character was added since.
    #[test]
    #[ignore]
    fn live_sources() {
        let d = refresh();
        eprintln!("news: {:?}\numalator: {:?}", d.news, d.umalator);
        assert!(d.news.ok && d.umalator.ok);
        let (live, bundled) = (fetch_uma_guide_tables().unwrap(), kb());
        assert_eq!((&live.affinity.points, &live.affinity.members), (&bundled.affinity.points, &bundled.affinity.members), "rerun tools/update_knowledge.py --only affinity if a character was added");
        let sparks: std::collections::HashMap<u32, crate::knowledge::SparkInfo> = live.sparks.unwrap().into_iter().map(|s| (s.id, s)).collect();
        assert_eq!(sparks, bundled.sparks, "the app reads the spark table as the updater writes it");
        assert_eq!(live.saddles.unwrap(), bundled.saddles, "and the win table");
        let merged = merge(&kb(), &kb().umalator_presets, &d);
        for e in merged.iter().filter(|e| e.date.as_str() >= "2026-10-01") {
            eprintln!("{} {} {} {:?} announced={} disabled={}", e.date, e.name, e.course_name, e.source, e.announced, e.disabled_skills.len());
        }
    }
}

// ---------------------------------------------------------------------------
// In the app

/// The app's live-data state.
#[derive(Default)]
pub(crate) struct LiveState {
    /// What the last refresh found (from the cache until a refresh finishes).
    pub data: Option<LiveData>,
    pub rx: Option<Receiver<LiveData>>,
    pub affinity_rx: Option<Receiver<Result<UmaGuideTables, String>>>,
    /// The bundled presets, merged again after every refresh and Reload.
    pub bundled: Vec<EventPreset>,
    /// Start a refresh on the next frame (there's no `Context` in `App::new`).
    pub start: bool,
    /// The refresh was asked for in Settings, so its result gets a toast.
    pub manual: bool,
}

/// `live_events` from settings.json, read once: 0 not read yet, 1 off, 2 on.
static ENABLED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether to fetch on launch (Settings → Data and updates); on unless turned off.
pub(crate) fn enabled() -> bool {
    use std::sync::atomic::Ordering;
    match ENABLED.load(Ordering::Relaxed) {
        0 => {
            let on = crate::settings::get("live_events").and_then(|v| v.as_bool()).unwrap_or(true);
            ENABLED.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
        v => v == 2,
    }
}

/// Turn fetching on launch on or off, and save it.
pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(if on { 2 } else { 1 }, std::sync::atomic::Ordering::Relaxed);
    crate::settings::set("live_events", serde_json::json!(on));
}

impl crate::App {
    /// Load the cache and decide whether to refresh. `UMALORD_LIVE=off` keeps to the bundled
    /// data, `cache` uses the cache without fetching (screenshots), `fetch` refreshes now.
    pub(crate) fn live_start(&mut self, mode: Option<&str>) {
        self.live.bundled = self.kb.umalator_presets.clone();
        if mode == Some("off") {
            return;
        }
        if let Some(d) = load_cache() {
            self.live.data = Some(d);
            self.apply_live();
        }
        let stale = self.live.data.as_ref().is_none_or(|d| now() - d.checked_at > STALE_SECS);
        self.live.start = match mode {
            Some("fetch") => true,
            Some("cache") => false,
            _ => enabled() && stale,
        };
    }

    /// Merge the live data into the event list.
    pub(crate) fn apply_live(&mut self) {
        if let Some(d) = &self.live.data {
            self.kb.umalator_presets = merge(&self.kb, &self.live.bundled, d);
            self.planner.key_memo.set(None);
            // A fetched affinity table replaces the bundled one when it's newer.
            if let Some(a) = d.affinity.as_ref().filter(|a| !a.is_empty() && a.fetched > self.kb.affinity.fetched) {
                self.kb.affinity = a.clone();
                // The spark and win tables came in the same download, so they're as new.
                if let Some(s) = d.sparks.as_ref().filter(|s| !s.is_empty()) {
                    self.kb.sparks = s.iter().map(|s| (s.id, s.clone())).collect();
                }
                if let Some(w) = d.saddles.as_ref().filter(|w| !w.is_empty()) {
                    self.kb.saddles = w.clone();
                }
            }
        }
    }

    /// Refresh now, in the background.
    pub(crate) fn refresh_live(&mut self, manual: bool) {
        if self.live.rx.is_none() {
            self.live.start = true;
            self.live.manual = manual;
        }
    }

    /// Start a pending refresh, and take a finished one.
    pub(crate) fn live_poll(&mut self, ctx: &eframe::egui::Context) {
        if std::mem::take(&mut self.live.start) && self.live.rx.is_none() {
            self.live.rx = Some(spawn_refresh(Some(ctx.clone())));
            if self.live.affinity_rx.is_none() && (self.live.manual || LiveData::affinity_due(self.live.data.as_ref(), &self.kb.affinity.fetched)) {
                self.live.affinity_rx = Some(spawn_affinity(ctx.clone()));
            }
        }
        self.affinity_poll();
        let Some(rx) = &self.live.rx else { return };
        let Ok(d) = rx.try_recv() else { return };
        self.live.rx = None;
        let d = d.keep_old(self.live.data.as_ref());
        save_cache(&d);
        let (news_ok, uma_ok) = (d.news.ok, d.umalator.ok);
        let news_err = d.news.message.clone();
        self.live.data = Some(d);
        self.apply_live();
        // The Ask tab's OpenCode reads the schedule from the digest.
        if std::env::var_os("UMALORD_PROFILE").is_none() {
            self.write_digest();
        }
        if std::mem::take(&mut self.live.manual) {
            use crate::ui_util::ToastKind;
            match (news_ok, uma_ok) {
                (true, true) => self.toast("Champions Meeting schedule updated from umamusume.com and umalator.app.", ToastKind::Success),
                (true, false) => self.toast("Updated from umamusume.com; umalator.app didn't answer, so its last presets stay.", ToastKind::Warn),
                (false, true) => self.toast(format!("umamusume.com didn't answer ({news_err}); updated Umalator's estimates only."), ToastKind::Warn),
                (false, false) => self.toast("Couldn't reach umamusume.com or umalator.app; using what was saved before.", ToastKind::Error),
            }
        }
    }

    /// Take the affinity tables when their download finishes, and keep them in the cache.
    fn affinity_poll(&mut self) {
        let Some(rx) = &self.live.affinity_rx else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.live.affinity_rx = None;
        let t = now();
        let d = self.live.data.get_or_insert_with(Default::default);
        d.affinity_tried = t;
        match result {
            Ok(g) => {
                let extra = match (&g.sparks, &g.saddles) {
                    (Some(s), Some(w)) => format!(", {} sparks, {} win records", s.len(), w.len()),
                    _ => ", but not its spark or win table".into(),
                };
                d.uma_guide = SourceStatus { ok: true, message: format!("affinity for {} characters{extra}", g.affinity.members.len()), fetched_at: Some(t) };
                d.affinity = Some(g.affinity);
                d.sparks = g.sparks.or(d.sparks.take());
                d.saddles = g.saddles.or(d.saddles.take());
            }
            Err(e) => {
                d.uma_guide.ok = false;
                d.uma_guide.message = e;
            }
        }
        save_cache(d);
        self.apply_live();
    }

    /// The status bar's line about the schedule: text, hover, and whether it's a warning.
    pub(crate) fn live_status(&self) -> (String, String, bool) {
        let sources = |d: &LiveData| {
            let line = |name: &str, s: &SourceStatus| {
                let when = s.fetched_at.map(|t| format!(", last fetched {}", fmt_ago(now() - t))).unwrap_or_default();
                format!("{name}: {} {}{when}", if s.ok { "✔" } else { "✖" }, s.message)
            };
            format!(
                "{}\n{}\n{}\nChecked {}. Settings → Data and updates refreshes it.",
                line("umamusume.com", &d.news),
                line("umalator.app", &d.umalator),
                if d.uma_guide.message.is_empty() { "uma.guide (affinity): not fetched yet".to_string() } else { line("uma.guide (affinity)", &d.uma_guide) },
                fmt_ago(now() - d.checked_at)
            )
        };
        if self.live.rx.is_some() {
            return ("📅 Checking the Champions Meeting schedule…".into(), String::new(), false);
        }
        let bundled_day = self.kb.umalator_built.as_deref().map(|b| b[..10.min(b.len())].to_string()).unwrap_or_else(|| "the bundled data".into());
        match &self.live.data {
            Some(d) if d.news.ok => (format!("📅 CM schedule: updated {} (umamusume.com)", fmt_ago(now() - d.checked_at)), sources(d), false),
            Some(d) => match d.news.fetched_at {
                Some(t) => (format!("📅 CM schedule offline: from {} (umamusume.com)", fmt_ago(now() - t)), sources(d), true),
                None => (format!("📅 CM schedule offline: using data from {bundled_day}"), sources(d), true),
            },
            None if enabled() => (format!("📅 CM schedule: using data from {bundled_day}"), "Not fetched yet.".into(), false),
            None => (
                format!("📅 CM schedule: using data from {bundled_day}"),
                "Fetching the live schedule is off (Settings → Data and updates).".into(),
                false,
            ),
        }
    }
}
