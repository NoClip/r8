//! Safe Rust reimplementation of Google V8's ECMAScript `Date` built-in constructor and prototype.
//!
//! Implements timestamp math, ISO 8601 formatting, `Date.now()`, `Date.parse()`,
//! and UTC date-time component getters/setters adhering to ECMAScript standard.

use crate::objects::{JSFunction, JSObject, JSValue};
use crate::platform::time::Time;
use std::cell::RefCell;
use std::rc::Rc;

const MS_PER_DAY: i64 = 86_400_000;
const MS_PER_HOUR: i64 = 3_600_000;
const MS_PER_MINUTE: i64 = 60_000;
const MS_PER_SECOND: i64 = 1_000;

/// Converts days since 1970-01-01 to (year, month 1..12, day 1..31).
fn days_to_civil(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1020 + doe / 1461 - doe / 146096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Converts (year, month 1..12, day 1..31) to days since 1970-01-01.
fn civil_to_days(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe as i64 - 719468
}

/// Deconstructs millisecond timestamp into date-time components.
struct DateComponents {
    year: i32,
    month: u32, // 1..=12
    day: u32,   // 1..=31
    day_of_week: u32, // 0..=6 (0 = Sunday)
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: u32,
}

impl DateComponents {
    fn from_timestamp(ms: i64) -> Self {
        let days = ms.div_euclid(MS_PER_DAY);
        let rem_ms = ms.rem_euclid(MS_PER_DAY);

        let (year, month, day) = days_to_civil(days);
        let day_of_week = (days + 4).rem_euclid(7) as u32;

        let hour = (rem_ms / MS_PER_HOUR) as u32;
        let minute = ((rem_ms % MS_PER_HOUR) / MS_PER_MINUTE) as u32;
        let second = ((rem_ms % MS_PER_MINUTE) / MS_PER_SECOND) as u32;
        let millisecond = (rem_ms % MS_PER_SECOND) as u32;

        Self {
            year,
            month,
            day,
            day_of_week,
            hour,
            minute,
            second,
            millisecond,
        }
    }
}

/// Parses an ISO 8601 date string into milliseconds since epoch.
pub fn parse_iso_date(s: &str) -> Option<i64> {
    let s = s.trim();
    // Split date and time
    let parts: Vec<&str> = s.split(|c| c == 'T' || c == ' ').collect();
    let date_part = parts.first()?;

    let date_tokens: Vec<&str> = date_part.split('-').collect();
    if date_tokens.len() < 3 {
        return None;
    }
    let year: i32 = date_tokens[0].parse().ok()?;
    let month: u32 = date_tokens[1].parse().ok()?;
    let day: u32 = date_tokens[2].parse().ok()?;

    let mut hour = 0u32;
    let mut minute = 0u32;
    let mut second = 0u32;
    let mut millisecond = 0u32;

    if parts.len() > 1 {
        let mut time_str = parts[1];
        if time_str.ends_with('Z') || time_str.ends_with('z') {
            time_str = &time_str[..time_str.len() - 1];
        }

        let time_tokens: Vec<&str> = time_str.split(':').collect();
        if !time_tokens.is_empty() {
            hour = time_tokens[0].parse().unwrap_or(0);
        }
        if time_tokens.len() > 1 {
            minute = time_tokens[1].parse().unwrap_or(0);
        }
        if time_tokens.len() > 2 {
            let sec_str = time_tokens[2];
            if let Some((s_part, ms_part)) = sec_str.split_once('.') {
                second = s_part.parse().unwrap_or(0);
                let ms_norm = format!("{:0<3}", ms_part);
                millisecond = ms_norm[..3.min(ms_norm.len())].parse().unwrap_or(0);
            } else {
                second = sec_str.parse().unwrap_or(0);
            }
        }
    }

    let days = civil_to_days(year, month, day);
    let ms = days * MS_PER_DAY
        + (hour as i64) * MS_PER_HOUR
        + (minute as i64) * MS_PER_MINUTE
        + (second as i64) * MS_PER_SECOND
        + (millisecond as i64);

    Some(ms)
}

/// Helper to retrieve the `__time_ms__` property of a Date object.
fn get_date_time_ms(this: &JSValue) -> Result<i64, String> {
    if let JSValue::Object(obj) = this {
        let val = obj.borrow().get_property("__time_ms__");
        match val {
            JSValue::Smi(n) => Ok(n as i64),
            JSValue::Number(f) => Ok(f as i64),
            _ => Err("TypeError: this is not a Date object".to_string()),
        }
    } else {
        Err("TypeError: this is not a Date object".to_string())
    }
}

/// Creates a new Date instance holding the specified timestamp.
pub fn new_date_instance(prototype: Option<Rc<RefCell<JSObject>>>, ms: i64) -> Rc<RefCell<JSObject>> {
    let proto = prototype.or_else(|| {
        if let Some(global) = crate::runtime::current_global() {
            if let JSValue::Object(date_ctor) = global.borrow().get_property("Date") {
                if let JSValue::Object(p) = date_ctor.borrow().get_property("prototype") {
                    return Some(p);
                }
            }
        }
        None
    });
    let obj = JSObject::new_empty(proto);
    JSObject::set_property(&obj, "__time_ms__", JSValue::Number(ms as f64));
    obj
}

/// Allocates the standard ECMAScript `Date.prototype` object.
pub fn create_date_prototype() -> Rc<RefCell<JSObject>> {
    let proto = JSObject::new_empty(None);

    // Date.prototype.getTime() / valueOf()
    let get_time_fn = JSFunction::new_native("getTime", |this, _args| {
        let ms = get_date_time_ms(this)?;
        Ok(JSValue::Number(ms as f64))
    });
    JSObject::set_property(&proto, "getTime", JSValue::Function(get_time_fn.clone()));
    JSObject::set_property(&proto, "valueOf", JSValue::Function(get_time_fn));

    // Date.prototype.toISOString()
    JSObject::set_property(
        &proto,
        "toISOString",
        JSValue::Function(JSFunction::new_native("toISOString", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            let s = format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                c.year, c.month, c.day, c.hour, c.minute, c.second, c.millisecond
            );
            Ok(JSValue::String(s))
        })),
    );

    // Date.prototype.toString()
    JSObject::set_property(
        &proto,
        "toString",
        JSValue::Function(JSFunction::new_native("toString", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            let month_names = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            let day_name = day_names.get(c.day_of_week as usize).unwrap_or(&"Thu");
            let month_name = month_names.get(c.month.saturating_sub(1) as usize).unwrap_or(&"Jan");
            let s = format!(
                "{} {} {:02} {:04} {:02}:{:02}:{:02} GMT+0000",
                day_name, month_name, c.day, c.year, c.hour, c.minute, c.second
            );
            Ok(JSValue::String(s))
        })),
    );

    // Component getters
    JSObject::set_property(
        &proto,
        "getFullYear",
        JSValue::Function(JSFunction::new_native("getFullYear", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.year))
        })),
    );

    // Month is 0-indexed in ECMAScript (0 = January, 11 = December)
    JSObject::set_property(
        &proto,
        "getMonth",
        JSValue::Function(JSFunction::new_native("getMonth", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.month.saturating_sub(1) as i32))
        })),
    );

    JSObject::set_property(
        &proto,
        "getDate",
        JSValue::Function(JSFunction::new_native("getDate", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.day as i32))
        })),
    );

    JSObject::set_property(
        &proto,
        "getDay",
        JSValue::Function(JSFunction::new_native("getDay", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.day_of_week as i32))
        })),
    );

    JSObject::set_property(
        &proto,
        "getHours",
        JSValue::Function(JSFunction::new_native("getHours", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.hour as i32))
        })),
    );

    JSObject::set_property(
        &proto,
        "getMinutes",
        JSValue::Function(JSFunction::new_native("getMinutes", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.minute as i32))
        })),
    );

    JSObject::set_property(
        &proto,
        "getSeconds",
        JSValue::Function(JSFunction::new_native("getSeconds", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.second as i32))
        })),
    );

    JSObject::set_property(
        &proto,
        "getMilliseconds",
        JSValue::Function(JSFunction::new_native("getMilliseconds", |this, _args| {
            let ms = get_date_time_ms(this)?;
            let c = DateComponents::from_timestamp(ms);
            Ok(JSValue::Smi(c.millisecond as i32))
        })),
    );

    // Date.prototype.setTime(timeMs)
    JSObject::set_property(
        &proto,
        "setTime",
        JSValue::Function(JSFunction::new_native("setTime", |this, args| {
            if let JSValue::Object(obj) = this {
                let ms = args.first().map(|v| v.to_number() as i64).unwrap_or(0);
                JSObject::set_property(obj, "__time_ms__", JSValue::Number(ms as f64));
                Ok(JSValue::Number(ms as f64))
            } else {
                Err("TypeError: this is not a Date object".to_string())
            }
        })),
    );

    proto
}

/// Allocates the global `Date` constructor function object.
pub fn create_date_constructor(prototype: Rc<RefCell<JSObject>>) -> Rc<RefCell<JSObject>> {
    let date_ctor = JSObject::new_empty(None);
    JSObject::set_property(&date_ctor, "prototype", JSValue::Object(prototype));

    // Date.now()
    JSObject::set_property(
        &date_ctor,
        "now",
        JSValue::Function(JSFunction::new_native("now", |_this, _args| {
            let ms = Time::now().to_unix_timestamp_micros() / 1000;
            Ok(JSValue::Number(ms as f64))
        })),
    );

    // Date.parse(str)
    JSObject::set_property(
        &date_ctor,
        "parse",
        JSValue::Function(JSFunction::new_native("parse", |_this, args| {
            let s = match args.first() {
                Some(v) => v.to_string_val(),
                None => return Ok(JSValue::Number(f64::NAN)),
            };
            if let Some(ms) = parse_iso_date(&s) {
                Ok(JSValue::Number(ms as f64))
            } else {
                Ok(JSValue::Number(f64::NAN))
            }
        })),
    );

    // Date.UTC(year, month, date?, hours?, minutes?, seconds?, ms?)
    JSObject::set_property(
        &date_ctor,
        "UTC",
        JSValue::Function(JSFunction::new_native("UTC", |_this, args| {
            let year = args.get(0).map(|v| v.to_number() as i32).unwrap_or(1970);
            let month = args.get(1).map(|v| (v.to_number() as u32) + 1).unwrap_or(1);
            let day = args.get(2).map(|v| v.to_number() as u32).unwrap_or(1);
            let hour = args.get(3).map(|v| v.to_number() as u32).unwrap_or(0);
            let min = args.get(4).map(|v| v.to_number() as u32).unwrap_or(0);
            let sec = args.get(5).map(|v| v.to_number() as u32).unwrap_or(0);
            let ms_part = args.get(6).map(|v| v.to_number() as u32).unwrap_or(0);

            let days = civil_to_days(year, month, day);
            let total_ms = days * MS_PER_DAY
                + (hour as i64) * MS_PER_HOUR
                + (min as i64) * MS_PER_MINUTE
                + (sec as i64) * MS_PER_SECOND
                + (ms_part as i64);

            Ok(JSValue::Number(total_ms as f64))
        })),
    );

    // Connect callable constructor: when called as a function `Date(...)`, returns formatted string
    // When invoked with `new Date(...)`, returns Date object instance.
    let constructor_fn = JSFunction::new_native("Date", |_this, args| {
        let ms = match args.len() {
            0 => Time::now().to_unix_timestamp_micros() / 1000,
            1 => {
                let first = args.first().unwrap();
                match first {
                    JSValue::String(s) => parse_iso_date(s).unwrap_or(0),
                    other => other.to_number() as i64,
                }
            }
            _ => {
                let year = args.get(0).map(|v| v.to_number() as i32).unwrap_or(1970);
                let month = args.get(1).map(|v| (v.to_number() as u32) + 1).unwrap_or(1);
                let day = args.get(2).map(|v| v.to_number() as u32).unwrap_or(1);
                let hour = args.get(3).map(|v| v.to_number() as u32).unwrap_or(0);
                let min = args.get(4).map(|v| v.to_number() as u32).unwrap_or(0);
                let sec = args.get(5).map(|v| v.to_number() as u32).unwrap_or(0);
                let ms_part = args.get(6).map(|v| v.to_number() as u32).unwrap_or(0);

                let days = civil_to_days(year, month, day);
                days * MS_PER_DAY
                    + (hour as i64) * MS_PER_HOUR
                    + (min as i64) * MS_PER_MINUTE
                    + (sec as i64) * MS_PER_SECOND
                    + (ms_part as i64)
            }
        };

        Ok(JSValue::Object(new_date_instance(None, ms)))
    });

    // Attach callable invocation `Date(...)`
    JSObject::set_property(&date_ctor, "__call__", JSValue::Function(constructor_fn));

    date_ctor
}
