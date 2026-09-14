//! Safe Rust reimplementation of Google V8's `Intl` internationalization subsystem.
//!
//! Implements ECMAScript Internationalization API (ECMA-402) in 100% Pure Safe Rust
//! without external ICU or C library dependencies:
//! - `Intl.NumberFormat`: Locale-sensitive number, currency, and percentage formatting.
//! - `Intl.DateTimeFormat`: Locale-sensitive calendar date and time formatting.
//! - `Intl.Collator`: Locale-sensitive string comparison and sorting.
//! - `Intl.getCanonicalLocales`: BCP 47 locale normalization.

use crate::objects::{JSArray, JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

// ─── BCP 47 Locale Representation ──────────────────────────────────────────

/// Parsed BCP 47 language tag representation.
#[derive(Clone, Debug, PartialEq)]
pub struct LocaleInfo {
    pub tag: String,
    pub language: String,
    pub region: Option<String>,
}

impl LocaleInfo {
    pub fn parse(tag: &str) -> Self {
        let clean = tag.trim().replace('_', "-");
        let parts: Vec<&str> = clean.split('-').collect();
        let language = parts.first().unwrap_or(&"en").to_lowercase();
        let region = parts.get(1).map(|r| r.to_uppercase());
        let canonical_tag = match &region {
            Some(r) => format!("{}-{}", language, r),
            None => language.clone(),
        };

        Self {
            tag: canonical_tag,
            language,
            region,
        }
    }

    pub fn default_locale() -> Self {
        Self::parse("en-US")
    }
}

// ─── Number Formatting Engine ───────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct NumberFormatOptions {
    pub locale: LocaleInfo,
    pub style: String, // "decimal", "currency", "percent"
    pub currency: Option<String>, // "USD", "EUR", "GBP", "JPY", etc.
    pub currency_display: String, // "symbol", "code", "name"
    pub use_grouping: bool,
    pub minimum_integer_digits: usize,
    pub minimum_fraction_digits: usize,
    pub maximum_fraction_digits: usize,
}

impl NumberFormatOptions {
    pub fn from_js(locale_str: &str, options_obj: Option<&Rc<RefCell<JSObject>>>) -> Self {
        let locale = if locale_str.is_empty() {
            LocaleInfo::default_locale()
        } else {
            LocaleInfo::parse(locale_str)
        };

        let mut style = "decimal".to_string();
        let mut currency = None;
        let mut currency_display = "symbol".to_string();
        let mut use_grouping = true;
        let mut min_integer_digits = 1;
        let mut min_fraction_digits = 0;
        let mut max_fraction_digits = 3;

        if let Some(opts) = options_obj {
            let o = opts.borrow();
            if let JSValue::String(st) = o.get_property("style") {
                style = st;
            }
            if let JSValue::String(cur) = o.get_property("currency") {
                currency = Some(cur.to_uppercase());
            }
            if let JSValue::String(d) = o.get_property("currencyDisplay") {
                currency_display = d;
            }
            if let JSValue::Boolean(b) = o.get_property("useGrouping") {
                use_grouping = b;
            }
            let mnd = o.get_property("minimumFractionDigits");
            if mnd != JSValue::Undefined {
                min_fraction_digits = mnd.to_number().max(0.0).min(20.0) as usize;
            }
            let mxd = o.get_property("maximumFractionDigits");
            if mxd != JSValue::Undefined {
                max_fraction_digits = mxd.to_number().max(0.0).min(20.0) as usize;
            }
            let mid = o.get_property("minimumIntegerDigits");
            if mid != JSValue::Undefined {
                min_integer_digits = mid.to_number().max(1.0).min(21.0) as usize;
            }
        }

        if style == "currency" {
            if currency.is_none() {
                currency = Some("USD".to_string());
            }
            if options_obj.is_none() || options_obj.unwrap().borrow().get_property("minimumFractionDigits") == JSValue::Undefined {
                min_fraction_digits = 2;
                max_fraction_digits = 2;
            }
        } else if style == "percent" {
            if options_obj.is_none() || options_obj.unwrap().borrow().get_property("maximumFractionDigits") == JSValue::Undefined {
                max_fraction_digits = 0;
            }
        }

        if min_fraction_digits > max_fraction_digits {
            max_fraction_digits = min_fraction_digits;
        }

        Self {
            locale,
            style,
            currency,
            currency_display,
            use_grouping,
            minimum_integer_digits: min_integer_digits,
            minimum_fraction_digits: min_fraction_digits,
            maximum_fraction_digits: max_fraction_digits,
        }
    }

    pub fn format(&self, mut val: f64) -> String {
        let is_negative = val < 0.0;
        if is_negative {
            val = -val;
        }

        if self.style == "percent" {
            val *= 100.0;
        }

        // Locale-specific separators
        let (group_sep, decimal_sep) = match self.locale.language.as_str() {
            "de" => ('.', ','),
            "fr" => (' ', ','),
            "es" => ('.', ','),
            "it" => ('.', ','),
            _ => (',', '.'),
        };

        // Split integer and fraction
        let factor = 10f64.powi(self.maximum_fraction_digits as i32);
        let rounded = (val * factor).round() / factor;

        let int_part = rounded.floor() as u64;
        let mut int_str = int_part.to_string();
        while int_str.len() < self.minimum_integer_digits {
            int_str.insert(0, '0');
        }

        if self.use_grouping && int_str.len() > 3 {
            let mut grouped = String::new();
            let chars: Vec<char> = int_str.chars().collect();
            let len = chars.len();
            for (idx, ch) in chars.iter().enumerate() {
                if idx > 0 && (len - idx) % 3 == 0 {
                    grouped.push(group_sep);
                }
                grouped.push(*ch);
            }
            int_str = grouped;
        }

        let mut formatted = int_str;

        if self.maximum_fraction_digits > 0 {
            let frac_val = (rounded - (rounded.floor())).abs();
            let mut frac_str = format!("{:.width$}", frac_val, width = self.maximum_fraction_digits);
            if let Some(dot_pos) = frac_str.find('.') {
                frac_str = frac_str[(dot_pos + 1)..].to_string();
            } else {
                frac_str = String::new();
            }

            // Trim excess zeros down to minimum_fraction_digits
            while frac_str.len() > self.minimum_fraction_digits && frac_str.ends_with('0') {
                frac_str.pop();
            }

            if !frac_str.is_empty() || self.minimum_fraction_digits > 0 {
                while frac_str.len() < self.minimum_fraction_digits {
                    frac_str.push('0');
                }
                formatted.push(decimal_sep);
                formatted.push_str(&frac_str);
            }
        }

        if is_negative {
            formatted.insert(0, '-');
        }

        match self.style.as_str() {
            "currency" => {
                let curr_code = self.currency.as_deref().unwrap_or("USD");
                let symbol = match curr_code {
                    "USD" => "$",
                    "EUR" => "€",
                    "GBP" => "£",
                    "JPY" => "¥",
                    _ => curr_code,
                };

                match self.locale.language.as_str() {
                    "de" | "fr" => format!("{} {}", formatted, symbol),
                    _ => format!("{}{}", symbol, formatted),
                }
            }
            "percent" => {
                format!("{}%", formatted)
            }
            _ => formatted,
        }
    }
}

// ─── DateTime Formatting Engine ─────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct DateTimeFormatOptions {
    pub locale: LocaleInfo,
    pub year: Option<String>,   // "numeric", "2-digit"
    pub month: Option<String>,  // "numeric", "2-digit", "short", "long"
    pub day: Option<String>,    // "numeric", "2-digit"
    pub hour: Option<String>,   // "numeric", "2-digit"
    pub minute: Option<String>, // "numeric", "2-digit"
    pub second: Option<String>, // "numeric", "2-digit"
    pub hour12: bool,
}

impl DateTimeFormatOptions {
    pub fn from_js(locale_str: &str, options_obj: Option<&Rc<RefCell<JSObject>>>) -> Self {
        let locale = if locale_str.is_empty() {
            LocaleInfo::default_locale()
        } else {
            LocaleInfo::parse(locale_str)
        };

        let mut year = Some("numeric".to_string());
        let mut month = Some("numeric".to_string());
        let mut day = Some("numeric".to_string());
        let mut hour = None;
        let mut minute = None;
        let mut second = None;
        let mut hour12 = locale.language != "de" && locale.language != "fr";

        if let Some(opts) = options_obj {
            let o = opts.borrow();
            let y_val = o.get_property("year");
            if let JSValue::String(s) = y_val { year = Some(s); }
            let m_val = o.get_property("month");
            if let JSValue::String(s) = m_val { month = Some(s); }
            let d_val = o.get_property("day");
            if let JSValue::String(s) = d_val { day = Some(s); }
            let h_val = o.get_property("hour");
            if let JSValue::String(s) = h_val { hour = Some(s); }
            let min_val = o.get_property("minute");
            if let JSValue::String(s) = min_val { minute = Some(s); }
            let sec_val = o.get_property("second");
            if let JSValue::String(s) = sec_val { second = Some(s); }
            if let JSValue::Boolean(b) = o.get_property("hour12") {
                hour12 = b;
            }
        }

        Self {
            locale,
            year,
            month,
            day,
            hour,
            minute,
            second,
            hour12,
        }
    }

    pub fn format(&self, timestamp_ms: i64) -> String {
        // Break timestamp down into civil components
        let days = timestamp_ms.div_euclid(86_400_000);
        let rem_ms = timestamp_ms.rem_euclid(86_400_000);

        // Civil day conversion
        let z = days + 719468;
        let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1020 + doe / 1461 - doe / 146096) / 365;
        let y = yoe as i32 + era as i32 * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year_val = if m <= 2 { y + 1 } else { y };
        let month_val = m;
        let day_val = d;

        let total_sec = rem_ms / 1000;
        let hour_val = (total_sec / 3600) as u32;
        let min_val = ((total_sec % 3600) / 60) as u32;
        let sec_val = (total_sec % 60) as u32;

        let month_str = match self.month.as_deref() {
            Some("2-digit") => format!("{:02}", month_val),
            Some("short") => match month_val {
                1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr", 5 => "May", 6 => "Jun",
                7 => "Jul", 8 => "Aug", 9 => "Sep", 10 => "Oct", 11 => "Nov", _ => "Dec",
            }.to_string(),
            Some("long") => match month_val {
                1 => "January", 2 => "February", 3 => "March", 4 => "April",
                5 => "May", 6 => "June", 7 => "July", 8 => "August",
                9 => "September", 10 => "October", 11 => "November", _ => "December",
            }.to_string(),
            _ => format!("{}", month_val),
        };

        let day_str = match self.day.as_deref() {
            Some("2-digit") => format!("{:02}", day_val),
            _ => format!("{}", day_val),
        };

        let year_str = match self.year.as_deref() {
            Some("2-digit") => format!("{:02}", (year_val % 100).abs()),
            _ => format!("{}", year_val),
        };

        let date_part = match self.locale.language.as_str() {
            "de" => format!("{}.{}.{}", day_str, month_str, year_str),
            "fr" | "es" | "it" => format!("{}/{}/{}", day_str, month_str, year_str),
            "ja" | "zh" => format!("{}/{}/{}", year_str, month_str, day_str),
            _ => format!("{}/{}/{}", month_str, day_str, year_str), // en-US default
        };

        if self.hour.is_some() || self.minute.is_some() {
            let (disp_h, ampm) = if self.hour12 {
                let h = if hour_val == 0 { 12 } else if hour_val > 12 { hour_val - 12 } else { hour_val };
                let ap = if hour_val >= 12 { " PM" } else { " AM" };
                (h, ap)
            } else {
                (hour_val, "")
            };

            let h_str = format!("{:02}", disp_h);
            let m_str = format!("{:02}", min_val);
            let time_part = if self.second.is_some() {
                format!("{}:{}:{:02}{}", h_str, m_str, sec_val, ampm)
            } else {
                format!("{}:{}{}", h_str, m_str, ampm)
            };

            format!("{}, {}", date_part, time_part)
        } else {
            date_part
        }
    }
}

// ─── Collator Engine ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct CollatorOptions {
    pub locale: LocaleInfo,
    pub sensitivity: String, // "base", "accent", "case", "variant"
    pub numeric: bool,
}

impl CollatorOptions {
    pub fn from_js(locale_str: &str, options_obj: Option<&Rc<RefCell<JSObject>>>) -> Self {
        let locale = if locale_str.is_empty() {
            LocaleInfo::default_locale()
        } else {
            LocaleInfo::parse(locale_str)
        };

        let mut sensitivity = "variant".to_string();
        let mut numeric = false;

        if let Some(opts) = options_obj {
            let o = opts.borrow();
            let s_val = o.get_property("sensitivity");
            if let JSValue::String(st) = s_val {
                sensitivity = st;
            }
            let n_val = o.get_property("numeric");
            if let JSValue::Boolean(b) = n_val {
                numeric = b;
            }
        }

        Self {
            locale,
            sensitivity,
            numeric,
        }
    }

    pub fn compare(&self, a: &str, b: &str) -> i32 {
        match self.sensitivity.as_str() {
            "base" => {
                let al = a.to_lowercase();
                let bl = b.to_lowercase();
                al.cmp(&bl) as i32
            }
            "case" => {
                a.cmp(b) as i32
            }
            _ => {
                a.cmp(b) as i32
            }
        }
    }
}

// ─── ECMAScript Factory Functions ───────────────────────────────────────────

/// Creates the `Intl` root object containing `NumberFormat`, `DateTimeFormat`, and `Collator`.
pub fn create_intl_object() -> Rc<RefCell<JSObject>> {
    let intl = JSObject::new_empty(None);

    // 1. Intl.NumberFormat constructor
    let number_format_ctor = JSFunction::new_native("NumberFormat", |_this, args| {
        let locale = args.first().map(|a| a.to_string_val()).unwrap_or_default();
        let options_obj = args.get(1).and_then(|a| match a {
            JSValue::Object(o) => Some(o.clone()),
            _ => None,
        });

        let opts = NumberFormatOptions::from_js(&locale, options_obj.as_ref());
        let instance = JSObject::new_empty(None);

        // format method
        let opts_clone = opts.clone();
        let format_fn = JSFunction::new_closure("format", move |_this, f_args| {
            let num = f_args.first().map(|a| a.to_number()).unwrap_or(0.0);
            Ok(JSValue::String(opts_clone.format(num)))
        });
        JSObject::set_property(&instance, "format", JSValue::Function(format_fn));

        // resolvedOptions method
        let resolved_locale = opts.locale.tag.clone();
        let resolved_style = opts.style.clone();
        let resolved_currency = opts.currency.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(resolved_locale.clone()));
            JSObject::set_property(&res, "style", JSValue::String(resolved_style.clone()));
            JSObject::set_property(&res, "numberingSystem", JSValue::String("latn".to_string()));
            if let Some(c) = &resolved_currency {
                JSObject::set_property(&res, "currency", JSValue::String(c.clone()));
            }
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "NumberFormat", JSValue::Function(number_format_ctor));

    // 2. Intl.DateTimeFormat constructor
    let date_time_format_ctor = JSFunction::new_native("DateTimeFormat", |_this, args| {
        let locale = args.first().map(|a| a.to_string_val()).unwrap_or_default();
        let options_obj = args.get(1).and_then(|a| match a {
            JSValue::Object(o) => Some(o.clone()),
            _ => None,
        });

        let opts = DateTimeFormatOptions::from_js(&locale, options_obj.as_ref());
        let instance = JSObject::new_empty(None);

        // format method
        let opts_clone = opts.clone();
        let format_fn = JSFunction::new_closure("format", move |_this, f_args| {
            let ts = if let Some(first) = f_args.first() {
                match first {
                    JSValue::Object(obj) => {
                        let prop = obj.borrow().get_property("_timestamp");
                        if prop != JSValue::Undefined {
                            prop.to_number() as i64
                        } else {
                            0
                        }
                    }
                    val => val.to_number() as i64,
                }
            } else {
                0
            };
            Ok(JSValue::String(opts_clone.format(ts)))
        });
        JSObject::set_property(&instance, "format", JSValue::Function(format_fn));

        // resolvedOptions method
        let resolved_locale = opts.locale.tag.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(resolved_locale.clone()));
            JSObject::set_property(&res, "calendar", JSValue::String("gregory".to_string()));
            JSObject::set_property(&res, "numberingSystem", JSValue::String("latn".to_string()));
            JSObject::set_property(&res, "timeZone", JSValue::String("UTC".to_string()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "DateTimeFormat", JSValue::Function(date_time_format_ctor));

    // 3. Intl.Collator constructor
    let collator_ctor = JSFunction::new_native("Collator", |_this, args| {
        let locale = args.first().map(|a| a.to_string_val()).unwrap_or_default();
        let options_obj = args.get(1).and_then(|a| match a {
            JSValue::Object(o) => Some(o.clone()),
            _ => None,
        });

        let opts = CollatorOptions::from_js(&locale, options_obj.as_ref());
        let instance = JSObject::new_empty(None);

        // compare method
        let opts_clone = opts.clone();
        let compare_fn = JSFunction::new_closure("compare", move |_this, c_args| {
            let s1 = c_args.first().map(|a| a.to_string_val()).unwrap_or_default();
            let s2 = c_args.get(1).map(|a| a.to_string_val()).unwrap_or_default();
            Ok(JSValue::Smi(opts_clone.compare(&s1, &s2)))
        });
        JSObject::set_property(&instance, "compare", JSValue::Function(compare_fn));

        let resolved_locale = opts.locale.tag.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(resolved_locale.clone()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "Collator", JSValue::Function(collator_ctor));

    // 4. Intl.getCanonicalLocales(locales)
    let get_canonical_locales_fn = JSFunction::new_native("getCanonicalLocales", |_this, args| {
        let mut list = Vec::new();
        if let Some(first) = args.first() {
            match first {
                JSValue::String(s) => {
                    list.push(JSValue::String(LocaleInfo::parse(s).tag));
                }
                JSValue::Object(obj) | JSValue::Array(obj) => {
                    let o = obj.borrow();
                    for el in &o.elements {
                        list.push(JSValue::String(LocaleInfo::parse(&el.to_string_val()).tag));
                    }
                }
                _ => {}
            }
        }
        Ok(JSValue::Array(JSArray::new_array(list)))
    });
    JSObject::set_property(&intl, "getCanonicalLocales", JSValue::Function(get_canonical_locales_fn));

    // 5. Intl.DisplayNames constructor (ECMA-402)
    let display_names_ctor = JSFunction::new_native("DisplayNames", |_this, args| {
        let locales = args.first().map(|a| a.to_string_val()).unwrap_or_else(|| "en".to_string());
        let options_obj = match args.get(1) {
            Some(JSValue::Object(o)) => o.clone(),
            _ => return Err("TypeError: Intl.DisplayNames options must be an object".to_string()),
        };
        let type_prop = options_obj.borrow().get_property("type");
        let display_type = match type_prop {
            JSValue::String(ref s) => s.clone(),
            _ => return Err("TypeError: Intl.DisplayNames options must have a type".to_string()),
        };
        let style = match options_obj.borrow().get_property("style") {
            JSValue::String(s) => s,
            _ => "long".to_string(),
        };
        let fallback = match options_obj.borrow().get_property("fallback") {
            JSValue::String(s) => s,
            _ => "code".to_string(),
        };
        let language_display = match options_obj.borrow().get_property("languageDisplay") {
            JSValue::String(s) => s,
            _ => "dialect".to_string(),
        };

        let locale_info = LocaleInfo::parse(&locales);
        let instance = JSObject::new_empty(None);

        let l_info = locale_info.clone();
        let d_type = display_type.clone();
        let d_fallback = fallback.clone();
        let of_fn = JSFunction::new_closure("of", move |_this, of_args| {
            let code = of_args.first().map(|a| a.to_string_val()).unwrap_or_default();
            let name = lookup_display_name(&l_info, &d_type, &code, &d_fallback);
            match name {
                Some(s) => Ok(JSValue::String(s)),
                None => Ok(JSValue::Undefined),
            }
        });
        JSObject::set_property(&instance, "of", JSValue::Function(of_fn));

        let res_locale = locale_info.tag.clone();
        let res_type = display_type.clone();
        let res_style = style.clone();
        let res_fallback = fallback.clone();
        let res_lang_disp = language_display.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(res_locale.clone()));
            JSObject::set_property(&res, "style", JSValue::String(res_style.clone()));
            JSObject::set_property(&res, "type", JSValue::String(res_type.clone()));
            JSObject::set_property(&res, "fallback", JSValue::String(res_fallback.clone()));
            JSObject::set_property(&res, "languageDisplay", JSValue::String(res_lang_disp.clone()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "DisplayNames", JSValue::Function(display_names_ctor));

    // 6. Intl.ListFormat constructor (ECMA-402)
    let list_format_ctor = JSFunction::new_native("ListFormat", |_this, args| {
        let locales = args.first().map(|a| a.to_string_val()).unwrap_or_else(|| "en".to_string());
        let l_info = LocaleInfo::parse(&locales);

        let mut ltype = "conjunction".to_string();
        let mut style = "long".to_string();
        if let Some(JSValue::Object(o)) = args.get(1) {
            let b = o.borrow();
            if let JSValue::String(t) = b.get_property("type") {
                ltype = t;
            }
            if let JSValue::String(s) = b.get_property("style") {
                style = s;
            }
        }

        let instance = JSObject::new_empty(None);

        let l_info_clone = l_info.clone();
        let ltype_clone = ltype.clone();
        let style_clone = style.clone();
        let format_fn = JSFunction::new_closure("format", move |_this, f_args| {
            let items = extract_string_list(f_args.first());
            Ok(JSValue::String(format_list(&l_info_clone, &ltype_clone, &style_clone, &items)))
        });
        JSObject::set_property(&instance, "format", JSValue::Function(format_fn));

        let l_info_clone2 = l_info.clone();
        let ltype_clone2 = ltype.clone();
        let style_clone2 = style.clone();
        let format_to_parts_fn = JSFunction::new_closure("formatToParts", move |_this, f_args| {
            let items = extract_string_list(f_args.first());
            let parts = format_list_to_parts(&l_info_clone2, &ltype_clone2, &style_clone2, &items);
            Ok(JSValue::Array(JSArray::new_array(parts)))
        });
        JSObject::set_property(&instance, "formatToParts", JSValue::Function(format_to_parts_fn));

        let res_loc = l_info.tag.clone();
        let res_type = ltype.clone();
        let res_style = style.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(res_loc.clone()));
            JSObject::set_property(&res, "type", JSValue::String(res_type.clone()));
            JSObject::set_property(&res, "style", JSValue::String(res_style.clone()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "ListFormat", JSValue::Function(list_format_ctor));

    // 7. Intl.PluralRules constructor (ECMA-402)
    let plural_rules_ctor = JSFunction::new_native("PluralRules", |_this, args| {
        let locales = args.first().map(|a| a.to_string_val()).unwrap_or_else(|| "en".to_string());
        let l_info = LocaleInfo::parse(&locales);

        let mut ptype = "cardinal".to_string();
        if let Some(JSValue::Object(o)) = args.get(1) {
            if let JSValue::String(t) = o.borrow().get_property("type") {
                ptype = t;
            }
        }

        let instance = JSObject::new_empty(None);

        let l_info_clone = l_info.clone();
        let ptype_clone = ptype.clone();
        let select_fn = JSFunction::new_closure("select", move |_this, s_args| {
            let n = s_args.first().map(|a| a.to_number()).unwrap_or(0.0);
            let cat = select_plural_category(&l_info_clone, &ptype_clone, n);
            Ok(JSValue::String(cat.to_string()))
        });
        JSObject::set_property(&instance, "select", JSValue::Function(select_fn));

        let l_info_clone2 = l_info.clone();
        let ptype_clone2 = ptype.clone();
        let select_range_fn = JSFunction::new_closure("selectRange", move |_this, r_args| {
            let start = r_args.first().map(|a| a.to_number()).unwrap_or(0.0);
            let end = r_args.get(1).map(|a| a.to_number()).unwrap_or(start);
            if (start - end).abs() < f64::EPSILON {
                let cat = select_plural_category(&l_info_clone2, &ptype_clone2, start);
                Ok(JSValue::String(cat.to_string()))
            } else {
                let cat = select_plural_category(&l_info_clone2, &ptype_clone2, end);
                Ok(JSValue::String(cat.to_string()))
            }
        });
        JSObject::set_property(&instance, "selectRange", JSValue::Function(select_range_fn));

        let res_loc = l_info.tag.clone();
        let res_ptype = ptype.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(res_loc.clone()));
            JSObject::set_property(&res, "type", JSValue::String(res_ptype.clone()));
            JSObject::set_property(&res, "minimumIntegerDigits", JSValue::Smi(1));
            JSObject::set_property(&res, "minimumFractionDigits", JSValue::Smi(0));
            JSObject::set_property(&res, "maximumFractionDigits", JSValue::Smi(3));
            let cats = vec![
                JSValue::String("one".to_string()),
                JSValue::String("other".to_string()),
            ];
            JSObject::set_property(&res, "pluralCategories", JSValue::Array(JSArray::new_array(cats)));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "PluralRules", JSValue::Function(plural_rules_ctor));

    // 8. Intl.RelativeTimeFormat constructor (ECMA-402)
    let relative_time_format_ctor = JSFunction::new_native("RelativeTimeFormat", |_this, args| {
        let locales = args.first().map(|a| a.to_string_val()).unwrap_or_else(|| "en".to_string());
        let l_info = LocaleInfo::parse(&locales);

        let mut numeric = "always".to_string();
        let mut style = "long".to_string();
        if let Some(JSValue::Object(o)) = args.get(1) {
            let b = o.borrow();
            if let JSValue::String(n) = b.get_property("numeric") {
                numeric = n;
            }
            if let JSValue::String(s) = b.get_property("style") {
                style = s;
            }
        }

        let instance = JSObject::new_empty(None);

        let l_info_clone = l_info.clone();
        let num_clone = numeric.clone();
        let format_fn = JSFunction::new_closure("format", move |_this, f_args| {
            let val = f_args.first().map(|a| a.to_number()).unwrap_or(0.0);
            let unit = f_args.get(1).map(|a| a.to_string_val()).unwrap_or_else(|| "day".to_string());
            Ok(JSValue::String(format_relative_time(&l_info_clone, &num_clone, val, &unit)))
        });
        JSObject::set_property(&instance, "format", JSValue::Function(format_fn));

        let l_info_clone2 = l_info.clone();
        let num_clone2 = numeric.clone();
        let format_to_parts_fn = JSFunction::new_closure("formatToParts", move |_this, f_args| {
            let val = f_args.first().map(|a| a.to_number()).unwrap_or(0.0);
            let unit = f_args.get(1).map(|a| a.to_string_val()).unwrap_or_else(|| "day".to_string());
            let parts = format_relative_time_to_parts(&l_info_clone2, &num_clone2, val, &unit);
            Ok(JSValue::Array(JSArray::new_array(parts)))
        });
        JSObject::set_property(&instance, "formatToParts", JSValue::Function(format_to_parts_fn));

        let res_loc = l_info.tag.clone();
        let res_style = style.clone();
        let res_num = numeric.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(res_loc.clone()));
            JSObject::set_property(&res, "style", JSValue::String(res_style.clone()));
            JSObject::set_property(&res, "numeric", JSValue::String(res_num.clone()));
            JSObject::set_property(&res, "numberingSystem", JSValue::String("latn".to_string()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "RelativeTimeFormat", JSValue::Function(relative_time_format_ctor));

    // 9. Intl.Segmenter constructor (ECMA-402)
    let segmenter_ctor = JSFunction::new_native("Segmenter", |_this, args| {
        let locales = args.first().map(|a| a.to_string_val()).unwrap_or_else(|| "en".to_string());
        let l_info = LocaleInfo::parse(&locales);
        let granularity = match args.get(1) {
            Some(JSValue::Object(o)) => match o.borrow().get_property("granularity") {
                JSValue::String(g) => g,
                _ => "grapheme".to_string(),
            },
            _ => "grapheme".to_string(),
        };

        let instance = JSObject::new_empty(None);

        let g_clone = granularity.clone();
        let segment_fn = JSFunction::new_closure("segment", move |_this, s_args| {
            let input = s_args.first().map(|a| a.to_string_val()).unwrap_or_default();
            let seg_items = segment_string(&input, &g_clone);

            let segments_obj = JSObject::new_empty(None);
            let mut arr_items = Vec::with_capacity(seg_items.len());

            for item in &seg_items {
                let s_obj = JSObject::new_empty(None);
                JSObject::set_property(&s_obj, "segment", JSValue::String(item.segment.clone()));
                JSObject::set_property(&s_obj, "index", JSValue::Smi(item.index as i32));
                JSObject::set_property(&s_obj, "input", JSValue::String(item.input.clone()));
                if let Some(w) = item.is_word_like {
                    JSObject::set_property(&s_obj, "isWordLike", JSValue::Boolean(w));
                }
                arr_items.push(JSValue::Object(s_obj));
            }

            let arr = JSArray::new_array(arr_items);

            let items_for_containing = seg_items.clone();
            let containing_fn = JSFunction::new_closure("containing", move |_this, c_args| {
                let idx = c_args.first().map(|a| a.to_number() as usize).unwrap_or(0);
                for it in &items_for_containing {
                    if it.index <= idx && idx < it.index + it.segment.len() {
                        let res = JSObject::new_empty(None);
                        JSObject::set_property(&res, "segment", JSValue::String(it.segment.clone()));
                        JSObject::set_property(&res, "index", JSValue::Smi(it.index as i32));
                        JSObject::set_property(&res, "input", JSValue::String(it.input.clone()));
                        if let Some(w) = it.is_word_like {
                            JSObject::set_property(&res, "isWordLike", JSValue::Boolean(w));
                        }
                        return Ok(JSValue::Object(res));
                    }
                }
                Ok(JSValue::Undefined)
            });
            JSObject::set_property(&segments_obj, "containing", JSValue::Function(containing_fn));

            let arr_clone = arr.clone();
            let iter_fn = JSFunction::new_closure("iterator", move |_this, _args| {
                Ok(crate::objects::generator::new_array_iterator(arr_clone.clone()))
            });
            JSObject::set_property(&segments_obj, "Symbol(Symbol.iterator)", JSValue::Function(iter_fn.clone()));
            JSObject::set_property(&segments_obj, "[Symbol.iterator]", JSValue::Function(iter_fn.clone()));
            JSObject::set_property(&segments_obj, "iterator", JSValue::Function(iter_fn));

            Ok(JSValue::Object(segments_obj))
        });
        JSObject::set_property(&instance, "segment", JSValue::Function(segment_fn));

        let res_loc = l_info.tag.clone();
        let res_gran = granularity.clone();
        let res_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(res_loc.clone()));
            JSObject::set_property(&res, "granularity", JSValue::String(res_gran.clone()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(res_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "Segmenter", JSValue::Function(segmenter_ctor));

    // 10. Intl.Locale constructor (ECMA-402)
    let locale_ctor = JSFunction::new_native("Locale", |_this, args| {
        let tag = args.first().map(|a| a.to_string_val()).unwrap_or_else(|| "en".to_string());
        let mut parsed = ParsedLocale::parse(&tag);

        if let Some(JSValue::Object(opts)) = args.get(1) {
            let b = opts.borrow();
            if let JSValue::String(s) = b.get_property("language") { parsed.language = s; }
            if let JSValue::String(s) = b.get_property("script") { parsed.script = Some(s); }
            if let JSValue::String(s) = b.get_property("region") { parsed.region = Some(s); }
            if let JSValue::String(s) = b.get_property("calendar") { parsed.calendar = Some(s); }
            if let JSValue::String(s) = b.get_property("collation") { parsed.collation = Some(s); }
            if let JSValue::String(s) = b.get_property("hourCycle") { parsed.hour_cycle = Some(s); }
            if let JSValue::String(s) = b.get_property("numberingSystem") { parsed.numbering_system = Some(s); }
            if let JSValue::Boolean(b_val) = b.get_property("numeric") { parsed.numeric = Some(b_val); }
            if let JSValue::String(s) = b.get_property("caseFirst") { parsed.case_first = Some(s); }
        }

        let instance = JSObject::new_empty(None);
        JSObject::set_property(&instance, "baseName", JSValue::String(parsed.base_name.clone()));
        JSObject::set_property(&instance, "language", JSValue::String(parsed.language.clone()));
        JSObject::set_property(&instance, "script", parsed.script.as_ref().map(|s| JSValue::String(s.clone())).unwrap_or(JSValue::Undefined));
        JSObject::set_property(&instance, "region", parsed.region.as_ref().map(|r| JSValue::String(r.clone())).unwrap_or(JSValue::Undefined));
        JSObject::set_property(&instance, "calendar", parsed.calendar.as_ref().map(|c| JSValue::String(c.clone())).unwrap_or(JSValue::Undefined));
        JSObject::set_property(&instance, "collation", parsed.collation.as_ref().map(|c| JSValue::String(c.clone())).unwrap_or(JSValue::Undefined));
        JSObject::set_property(&instance, "hourCycle", parsed.hour_cycle.as_ref().map(|h| JSValue::String(h.clone())).unwrap_or(JSValue::Undefined));
        JSObject::set_property(&instance, "numberingSystem", parsed.numbering_system.as_ref().map(|n| JSValue::String(n.clone())).unwrap_or(JSValue::Undefined));
        JSObject::set_property(&instance, "numeric", parsed.numeric.map(JSValue::Boolean).unwrap_or(JSValue::Undefined));
        JSObject::set_property(&instance, "caseFirst", parsed.case_first.as_ref().map(|c| JSValue::String(c.clone())).unwrap_or(JSValue::Undefined));

        let p_max = parsed.maximize();
        let max_fn = JSFunction::new_closure("maximize", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "baseName", JSValue::String(p_max.clone()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "maximize", JSValue::Function(max_fn));

        let p_min = parsed.minimize();
        let min_fn = JSFunction::new_closure("minimize", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "baseName", JSValue::String(p_min.clone()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "minimize", JSValue::Function(min_fn));

        let tag_str = parsed.canonical_tag.clone();
        let to_str_fn = JSFunction::new_closure("toString", move |_this, _args| {
            Ok(JSValue::String(tag_str.clone()))
        });
        JSObject::set_property(&instance, "toString", JSValue::Function(to_str_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "Locale", JSValue::Function(locale_ctor));

    // 11. Intl.DurationFormat constructor (ECMA-402 ES2024)
    let duration_format_ctor = JSFunction::new_native("DurationFormat", |_this, args| {
        let locales = args.first().map(|a| a.to_string_val()).unwrap_or_else(|| "en".to_string());
        let l_info = LocaleInfo::parse(&locales);

        let mut style = "short".to_string();
        if let Some(JSValue::Object(o)) = args.get(1) {
            if let JSValue::String(s) = o.borrow().get_property("style") {
                style = s;
            }
        }

        let instance = JSObject::new_empty(None);

        let style_clone = style.clone();
        let format_fn = JSFunction::new_closure("format", move |_this, f_args| {
            let record = DurationRecord::from_js(f_args.first().unwrap_or(&JSValue::Undefined));
            Ok(JSValue::String(format_duration(&style_clone, &record)))
        });
        JSObject::set_property(&instance, "format", JSValue::Function(format_fn));

        let style_clone2 = style.clone();
        let format_to_parts_fn = JSFunction::new_closure("formatToParts", move |_this, f_args| {
            let record = DurationRecord::from_js(f_args.first().unwrap_or(&JSValue::Undefined));
            let parts = format_duration_to_parts(&style_clone2, &record);
            Ok(JSValue::Array(JSArray::new_array(parts)))
        });
        JSObject::set_property(&instance, "formatToParts", JSValue::Function(format_to_parts_fn));

        let res_loc = l_info.tag.clone();
        let res_style = style.clone();
        let resolved_opts_fn = JSFunction::new_closure("resolvedOptions", move |_this, _args| {
            let res = JSObject::new_empty(None);
            JSObject::set_property(&res, "locale", JSValue::String(res_loc.clone()));
            JSObject::set_property(&res, "style", JSValue::String(res_style.clone()));
            JSObject::set_property(&res, "numberingSystem", JSValue::String("latn".to_string()));
            Ok(JSValue::Object(res))
        });
        JSObject::set_property(&instance, "resolvedOptions", JSValue::Function(resolved_opts_fn));

        Ok(JSValue::Object(instance))
    });
    JSObject::set_property(&intl, "DurationFormat", JSValue::Function(duration_format_ctor));

    intl
}

// ─── ECMA-402 Helper Functions & Data Structures ──────────────────────────────

fn extract_string_list(arg: Option<&JSValue>) -> Vec<String> {
    match arg {
        Some(JSValue::Array(arr)) => {
            arr.borrow().elements.iter().map(|e| e.to_string_val()).collect()
        }
        Some(JSValue::Object(obj)) => {
            let b = obj.borrow();
            let mut list = Vec::new();
            for el in &b.elements {
                list.push(el.to_string_val());
            }
            list
        }
        Some(val) => vec![val.to_string_val()],
        None => Vec::new(),
    }
}

fn lookup_display_name(locale: &LocaleInfo, dtype: &str, code: &str, fallback: &str) -> Option<String> {
    let lang = locale.language.to_lowercase();
    match dtype {
        "region" => {
            let region_up = code.to_uppercase();
            let name = match (region_up.as_str(), lang.as_str()) {
                ("US", "fr") => "États-Unis",
                ("US", "de") => "Vereinigte Staaten",
                ("US", "es") => "Estados Unidos",
                ("US", "zh") => "美国",
                ("US", "ja") => "アメリカ合衆国",
                ("US", _) => "United States",

                ("GB", "fr") => "Royaume-Uni",
                ("GB", "de") => "Vereinigtes Königreich",
                ("GB", "es") => "Reino Unido",
                ("GB", "zh") => "英国",
                ("GB", "ja") => "イギリス",
                ("GB", _) => "United Kingdom",

                ("FR", "de") => "Frankreich",
                ("FR", "es") => "Francia",
                ("FR", "zh") => "法国",
                ("FR", "ja") => "フランス",
                ("FR", _) => "France",

                ("DE", "fr") => "Allemagne",
                ("DE", "de") => "Deutschland",
                ("DE", "es") => "Alemania",
                ("DE", "zh") => "德国",
                ("DE", "ja") => "ドイツ",
                ("DE", _) => "Germany",

                ("CN", "fr") => "Chine",
                ("CN", "de") => "China",
                ("CN", "es") => "China",
                ("CN", "zh") => "中国",
                ("CN", "ja") => "中国",
                ("CN", _) => "China",

                ("JP", "fr") => "Japon",
                ("JP", "de") => "Japan",
                ("JP", "es") => "Japón",
                ("JP", "zh") => "日本",
                ("JP", "ja") => "日本",
                ("JP", _) => "Japan",

                ("ES", "fr") => "Espagne",
                ("ES", "de") => "Spanien",
                ("ES", "es") => "España",
                ("ES", "zh") => "西班牙",
                ("ES", "ja") => "スペイン",
                ("ES", _) => "Spain",

                ("IT", "fr") => "Italie",
                ("IT", "de") => "Italien",
                ("IT", "es") => "Italia",
                ("IT", "zh") => "意大利",
                ("IT", "ja") => "イタリア",
                ("IT", _) => "Italy",

                ("CA", "fr") => "Canada",
                ("CA", "de") => "Kanada",
                ("CA", "es") => "Canadá",
                ("CA", "zh") => "加拿大",
                ("CA", "ja") => "カナダ",
                ("CA", _) => "Canada",

                ("AU", "fr") => "Australie",
                ("AU", "de") => "Australien",
                ("AU", "es") => "Australia",
                ("AU", "zh") => "澳大利亚",
                ("AU", "ja") => "オーストラリア",
                ("AU", _) => "Australia",

                ("BR", "fr") => "Brésil",
                ("BR", "de") => "Brasilien",
                ("BR", "es") => "Brasil",
                ("BR", "zh") => "巴西",
                ("BR", "ja") => "ブラジル",
                ("BR", _) => "Brazil",

                ("RU", "fr") => "Russie",
                ("RU", "de") => "Russland",
                ("RU", "es") => "Rusia",
                ("RU", "zh") => "俄罗斯",
                ("RU", "ja") => "ロシア",
                ("RU", _) => "Russia",

                ("IN", "fr") => "Inde",
                ("IN", "de") => "Indien",
                ("IN", "es") => "India",
                ("IN", "zh") => "印度",
                ("IN", "ja") => "インド",
                ("IN", _) => "India",

                _ => {
                    if fallback == "code" {
                        return Some(code.to_string());
                    } else {
                        return None;
                    }
                }
            };
            Some(name.to_string())
        }
        "language" => {
            let lang_low = code.to_lowercase();
            let name = match (lang_low.as_str(), lang.as_str()) {
                ("en", "fr") => "anglais",
                ("en", "de") => "Englisch",
                ("en", "es") => "inglés",
                ("en", "zh") => "英语",
                ("en", "ja") => "英語",
                ("en", _) => "English",

                ("fr", "fr") => "français",
                ("fr", "de") => "Französisch",
                ("fr", "es") => "francés",
                ("fr", "zh") => "法语",
                ("fr", "ja") => "フランス語",
                ("fr", _) => "French",

                ("de", "fr") => "allemand",
                ("de", "de") => "Deutsch",
                ("de", "es") => "alemán",
                ("de", "zh") => "德语",
                ("de", "ja") => "ドイツ語",
                ("de", _) => "German",

                ("es", "fr") => "espagnol",
                ("es", "de") => "Spanisch",
                ("es", "es") => "español",
                ("es", "zh") => "西班牙语",
                ("es", "ja") => "スペイン語",
                ("es", _) => "Spanish",

                ("zh", "fr") => "chinois",
                ("zh", "de") => "Chinesisch",
                ("zh", "es") => "chino",
                ("zh", "zh") => "中文",
                ("zh", "ja") => "中国語",
                ("zh", _) => "Chinese",

                ("ja", "fr") => "japonais",
                ("ja", "de") => "Japanisch",
                ("ja", "es") => "japonés",
                ("ja", "zh") => "日语",
                ("ja", "ja") => "日本語",
                ("ja", _) => "Japanese",

                ("ru", "fr") => "russe",
                ("ru", "de") => "Russisch",
                ("ru", "es") => "ruso",
                ("ru", "zh") => "俄语",
                ("ru", "ja") => "ロシア語",
                ("ru", _) => "Russian",

                ("it", "fr") => "italien",
                ("it", "de") => "Italienisch",
                ("it", "es") => "italiano",
                ("it", "zh") => "意大利语",
                ("it", "ja") => "イタリア語",
                ("it", _) => "Italian",

                ("pt", "fr") => "portugais",
                ("pt", "de") => "Portugiesisch",
                ("pt", "es") => "portugués",
                ("pt", "zh") => "葡萄牙语",
                ("pt", "ja") => "ポルトガル語",
                ("pt", _) => "Portuguese",

                _ => {
                    if fallback == "code" {
                        return Some(code.to_string());
                    } else {
                        return None;
                    }
                }
            };
            Some(name.to_string())
        }
        "currency" => {
            let cur_up = code.to_uppercase();
            let name = match (cur_up.as_str(), lang.as_str()) {
                ("USD", "fr") => "dollar des États-Unis",
                ("USD", "de") => "US-Dollar",
                ("USD", "zh") => "美元",
                ("USD", "ja") => "米ドル",
                ("USD", _) => "US Dollar",

                ("EUR", "fr") => "euro",
                ("EUR", "de") => "Euro",
                ("EUR", "zh") => "欧元",
                ("EUR", "ja") => "ユーロ",
                ("EUR", _) => "Euro",

                ("GBP", "fr") => "livre sterling",
                ("GBP", "de") => "Britisches Pfund",
                ("GBP", "zh") => "英镑",
                ("GBP", "ja") => "英国ポンド",
                ("GBP", _) => "British Pound",

                ("JPY", "fr") => "yen japonais",
                ("JPY", "de") => "Japanischer Yen",
                ("JPY", "zh") => "日元",
                ("JPY", "ja") => "日本円",
                ("JPY", _) => "Japanese Yen",

                ("CNY", "fr") => "yuan renminbi chinois",
                ("CNY", "de") => "Chinesischer Yuan",
                ("CNY", "zh") => "人民币",
                ("CNY", "ja") => "中国人民元",
                ("CNY", _) => "Chinese Yuan",

                _ => {
                    if fallback == "code" {
                        return Some(code.to_string());
                    } else {
                        return None;
                    }
                }
            };
            Some(name.to_string())
        }
        "calendar" => {
            let cal_low = code.to_lowercase();
            let name = match cal_low.as_str() {
                "gregory" => "Gregorian Calendar",
                "chinese" => "Chinese Calendar",
                "islamic" => "Islamic Calendar",
                "hebrew" => "Hebrew Calendar",
                "buddhist" => "Buddhist Calendar",
                "japanese" => "Japanese Calendar",
                _ => {
                    if fallback == "code" {
                        return Some(code.to_string());
                    } else {
                        return None;
                    }
                }
            };
            Some(name.to_string())
        }
        "dateTimeField" => {
            let name = match code {
                "era" => "era",
                "year" => "year",
                "quarter" => "quarter",
                "month" => "month",
                "weekOfYear" => "week",
                "day" => "day",
                "hour" => "hour",
                "minute" => "minute",
                "second" => "second",
                "timeZoneName" => "time zone",
                _ => {
                    if fallback == "code" {
                        return Some(code.to_string());
                    } else {
                        return None;
                    }
                }
            };
            Some(name.to_string())
        }
        _ => {
            if fallback == "code" {
                Some(code.to_string())
            } else {
                None
            }
        }
    }
}

fn format_list(locale: &LocaleInfo, ltype: &str, style: &str, items: &[String]) -> String {
    if items.is_empty() {
        return String::new();
    }
    if items.len() == 1 {
        return items[0].clone();
    }
    let lang = locale.language.to_lowercase();
    if items.len() == 2 {
        let (first, second) = (&items[0], &items[1]);
        return match ltype {
            "disjunction" => match lang.as_str() {
                "fr" => format!("{} ou {}", first, second),
                "de" => format!("{} oder {}", first, second),
                "es" => format!("{} o {}", first, second),
                "zh" => format!("{}或{}", first, second),
                _ => format!("{} or {}", first, second),
            },
            "unit" => match style {
                "narrow" => format!("{} {}", first, second),
                _ => format!("{}, {}", first, second),
            },
            _ => match lang.as_str() {
                "fr" => format!("{} et {}", first, second),
                "de" => format!("{} und {}", first, second),
                "es" => format!("{} y {}", first, second),
                "zh" => format!("{}和{}", first, second),
                _ => format!("{} and {}", first, second),
            },
        };
    }

    let head = &items[..items.len() - 1];
    let last = items.last().unwrap();
    match ltype {
        "disjunction" => match lang.as_str() {
            "fr" => format!("{}, ou {}", head.join(", "), last),
            "de" => format!("{}, oder {}", head.join(", "), last),
            "es" => format!("{}, o {}", head.join(", "), last),
            "zh" => format!("{}或{}", head.join("、"), last),
            _ => format!("{}, or {}", head.join(", "), last),
        },
        "unit" => head.join(", ") + ", " + last,
        _ => match lang.as_str() {
            "fr" => format!("{}, et {}", head.join(", "), last),
            "de" => format!("{}, und {}", head.join(", "), last),
            "es" => format!("{}, y {}", head.join(", "), last),
            "zh" => format!("{}和{}", head.join("、"), last),
            _ => format!("{}, and {}", head.join(", "), last),
        },
    }
}

fn format_list_to_parts(locale: &LocaleInfo, ltype: &str, style: &str, items: &[String]) -> Vec<JSValue> {
    let mut parts = Vec::new();
    if items.is_empty() {
        return parts;
    }
    if items.len() == 1 {
        let p = JSObject::new_empty(None);
        JSObject::set_property(&p, "type", JSValue::String("element".to_string()));
        JSObject::set_property(&p, "value", JSValue::String(items[0].clone()));
        parts.push(JSValue::Object(p));
        return parts;
    }

    let lang = locale.language.to_lowercase();
    if items.len() == 2 {
        let p0 = JSObject::new_empty(None);
        JSObject::set_property(&p0, "type", JSValue::String("element".to_string()));
        JSObject::set_property(&p0, "value", JSValue::String(items[0].clone()));
        parts.push(JSValue::Object(p0));

        let sep = match ltype {
            "disjunction" => match lang.as_str() {
                "fr" => " ou ",
                "de" => " oder ",
                "es" => " o ",
                "zh" => "或",
                _ => " or ",
            },
            "unit" => match style {
                "narrow" => " ",
                _ => ", ",
            },
            _ => match lang.as_str() {
                "fr" => " et ",
                "de" => " und ",
                "es" => " y ",
                "zh" => "和",
                _ => " and ",
            },
        };

        let lit = JSObject::new_empty(None);
        JSObject::set_property(&lit, "type", JSValue::String("literal".to_string()));
        JSObject::set_property(&lit, "value", JSValue::String(sep.to_string()));
        parts.push(JSValue::Object(lit));

        let p1 = JSObject::new_empty(None);
        JSObject::set_property(&p1, "type", JSValue::String("element".to_string()));
        JSObject::set_property(&p1, "value", JSValue::String(items[1].clone()));
        parts.push(JSValue::Object(p1));
        return parts;
    }

    for (i, item) in items.iter().enumerate() {
        let p = JSObject::new_empty(None);
        JSObject::set_property(&p, "type", JSValue::String("element".to_string()));
        JSObject::set_property(&p, "value", JSValue::String(item.clone()));
        parts.push(JSValue::Object(p));

        if i < items.len() - 1 {
            let lit = JSObject::new_empty(None);
            JSObject::set_property(&lit, "type", JSValue::String("literal".to_string()));
            if i == items.len() - 2 {
                let sep = match ltype {
                    "disjunction" => match lang.as_str() {
                        "fr" => ", ou ",
                        "de" => ", oder ",
                        "es" => ", o ",
                        "zh" => "或",
                        _ => ", or ",
                    },
                    "unit" => ", ",
                    _ => match lang.as_str() {
                        "fr" => ", et ",
                        "de" => ", und ",
                        "es" => ", y ",
                        "zh" => "和",
                        _ => ", and ",
                    },
                };
                JSObject::set_property(&lit, "value", JSValue::String(sep.to_string()));
            } else {
                let comma = if lang == "zh" { "、" } else { ", " };
                JSObject::set_property(&lit, "value", JSValue::String(comma.to_string()));
            }
            parts.push(JSValue::Object(lit));
        }
    }

    parts
}

fn select_plural_category(locale: &LocaleInfo, ptype: &str, n: f64) -> &'static str {
    let lang = locale.language.to_lowercase();
    if ptype == "ordinal" {
        if lang == "en" {
            let i = n.abs() as i64;
            let mod10 = i % 10;
            let mod100 = i % 100;
            if mod10 == 1 && mod100 != 11 {
                return "one";
            } else if mod10 == 2 && mod100 != 12 {
                return "two";
            } else if mod10 == 3 && mod100 != 13 {
                return "few";
            } else {
                return "other";
            }
        }
        return "other";
    }

    // Cardinal
    match lang.as_str() {
        "fr" => {
            if n >= 0.0 && n <= 1.5 {
                "one"
            } else {
                "other"
            }
        }
        "ru" => {
            let i = n.abs() as i64;
            if n.fract() == 0.0 {
                let mod10 = i % 10;
                let mod100 = i % 100;
                if mod10 == 1 && mod100 != 11 {
                    "one"
                } else if (2..=4).contains(&mod10) && !(12..=14).contains(&mod100) {
                    "few"
                } else if mod10 == 0 || (5..=9).contains(&mod10) || (11..=14).contains(&mod100) {
                    "many"
                } else {
                    "other"
                }
            } else {
                "other"
            }
        }
        "ar" => {
            let i = n.abs() as i64;
            if n == 0.0 {
                "zero"
            } else if n == 1.0 {
                "one"
            } else if n == 2.0 {
                "two"
            } else if (3..=10).contains(&(i % 100)) {
                "few"
            } else if (11..=99).contains(&(i % 100)) {
                "many"
            } else {
                "other"
            }
        }
        "zh" | "ja" | "ko" => "other",
        _ => {
            if (n - 1.0).abs() < f64::EPSILON {
                "one"
            } else {
                "other"
            }
        }
    }
}

fn format_relative_time(locale: &LocaleInfo, numeric: &str, val: f64, unit: &str) -> String {
    let clean_unit = unit.trim_end_matches('s').to_lowercase();
    let lang = locale.language.to_lowercase();
    let i_val = val as i64;

    if numeric == "auto" {
        if i_val == 0 {
            match clean_unit.as_str() {
                "day" => return match lang.as_str() { "fr" => "aujourd'hui", "de" => "heute", "zh" => "今天", _ => "today" }.to_string(),
                "week" => return match lang.as_str() { "fr" => "cette semaine", "de" => "diese Woche", "zh" => "本周", _ => "this week" }.to_string(),
                "month" => return match lang.as_str() { "fr" => "ce mois-ci", "de" => "diesen Monat", "zh" => "本月", _ => "this month" }.to_string(),
                "year" => return match lang.as_str() { "fr" => "cette année", "de" => "dieses Jahr", "zh" => "今年", _ => "this year" }.to_string(),
                "second" | "minute" | "hour" => return match lang.as_str() { "fr" => "maintenant", "de" => "jetzt", "zh" => "现在", _ => "now" }.to_string(),
                _ => {}
            }
        } else if i_val == 1 {
            match clean_unit.as_str() {
                "day" => return match lang.as_str() { "fr" => "demain", "de" => "morgen", "zh" => "明天", _ => "tomorrow" }.to_string(),
                "week" => return match lang.as_str() { "fr" => "la semaine prochaine", "de" => "nächste Woche", "zh" => "下周", _ => "next week" }.to_string(),
                "month" => return match lang.as_str() { "fr" => "le mois prochain", "de" => "nächsten Monat", "zh" => "下月", _ => "next month" }.to_string(),
                "year" => return match lang.as_str() { "fr" => "l'année prochaine", "de" => "nächstes Jahr", "zh" => "明年", _ => "next year" }.to_string(),
                _ => {}
            }
        } else if i_val == -1 {
            match clean_unit.as_str() {
                "day" => return match lang.as_str() { "fr" => "hier", "de" => "gestern", "zh" => "昨天", _ => "yesterday" }.to_string(),
                "week" => return match lang.as_str() { "fr" => "la semaine dernière", "de" => "letzte Woche", "zh" => "上周", _ => "last week" }.to_string(),
                "month" => return match lang.as_str() { "fr" => "le mois dernier", "de" => "letzten Monat", "zh" => "上月", _ => "last month" }.to_string(),
                "year" => return match lang.as_str() { "fr" => "l'année dernière", "de" => "letztes Jahr", "zh" => "去年", _ => "last year" }.to_string(),
                _ => {}
            }
        }
    }

    let abs_val = val.abs();
    let is_plural = (abs_val - 1.0).abs() > f64::EPSILON;
    let plural_s = if is_plural { "s" } else { "" };

    if val > 0.0 {
        match lang.as_str() {
            "fr" => format!("dans {} {}", val, clean_unit),
            "de" => format!("in {} {}", val, clean_unit),
            "es" => format!("en {} {}", val, clean_unit),
            "zh" => format!("{}{}后", val, clean_unit),
            _ => format!("in {} {}{}", val, clean_unit, plural_s),
        }
    } else if val < 0.0 {
        match lang.as_str() {
            "fr" => format!("il y a {} {}", abs_val, clean_unit),
            "de" => format!("vor {} {}", abs_val, clean_unit),
            "es" => format!("hace {} {}", abs_val, clean_unit),
            "zh" => format!("{}{}前", abs_val, clean_unit),
            _ => format!("{} {}{} ago", abs_val, clean_unit, plural_s),
        }
    } else {
        format!("in 0 {}s", clean_unit)
    }
}

fn format_relative_time_to_parts(locale: &LocaleInfo, numeric: &str, val: f64, unit: &str) -> Vec<JSValue> {
    let text = format_relative_time(locale, numeric, val, unit);
    let mut parts = Vec::new();
    let clean_unit = unit.trim_end_matches('s').to_lowercase();
    let abs_val_str = format!("{}", val.abs());

    if let Some(idx) = text.find(&abs_val_str) {
        if idx > 0 {
            let part1 = JSObject::new_empty(None);
            JSObject::set_property(&part1, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&part1, "value", JSValue::String(text[..idx].to_string()));
            parts.push(JSValue::Object(part1));
        }
        let int_part = JSObject::new_empty(None);
        JSObject::set_property(&int_part, "type", JSValue::String("integer".to_string()));
        JSObject::set_property(&int_part, "value", JSValue::String(abs_val_str.clone()));
        JSObject::set_property(&int_part, "unit", JSValue::String(clean_unit.clone()));
        parts.push(JSValue::Object(int_part));

        let rest_idx = idx + abs_val_str.len();
        if rest_idx < text.len() {
            let rest_part = JSObject::new_empty(None);
            JSObject::set_property(&rest_part, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&rest_part, "value", JSValue::String(text[rest_idx..].to_string()));
            parts.push(JSValue::Object(rest_part));
        }
    } else {
        let p = JSObject::new_empty(None);
        JSObject::set_property(&p, "type", JSValue::String("literal".to_string()));
        JSObject::set_property(&p, "value", JSValue::String(text));
        parts.push(JSValue::Object(p));
    }
    parts
}

#[derive(Clone, Debug)]
struct SegmentItem {
    segment: String,
    index: usize,
    input: String,
    is_word_like: Option<bool>,
}

fn segment_string(input: &str, granularity: &str) -> Vec<SegmentItem> {
    let mut results = Vec::new();
    if input.is_empty() {
        return results;
    }
    match granularity {
        "word" => {
            let mut current_segment = String::new();
            let mut current_index = 0;
            let mut in_word = false;

            for (idx, ch) in input.char_indices() {
                let is_alnum = ch.is_alphanumeric();
                if current_segment.is_empty() {
                    current_index = idx;
                    in_word = is_alnum;
                    current_segment.push(ch);
                } else if is_alnum == in_word {
                    current_segment.push(ch);
                } else {
                    results.push(SegmentItem {
                        segment: current_segment,
                        index: current_index,
                        input: input.to_string(),
                        is_word_like: Some(in_word),
                    });
                    current_segment = String::new();
                    current_index = idx;
                    in_word = is_alnum;
                    current_segment.push(ch);
                }
            }
            if !current_segment.is_empty() {
                results.push(SegmentItem {
                    segment: current_segment,
                    index: current_index,
                    input: input.to_string(),
                    is_word_like: Some(in_word),
                });
            }
        }
        "sentence" => {
            let mut current_segment = String::new();
            let mut current_index = 0;
            let chars: Vec<(usize, char)> = input.char_indices().collect();
            let len = chars.len();

            for i in 0..len {
                let (idx, ch) = chars[i];
                if current_segment.is_empty() {
                    current_index = idx;
                }
                current_segment.push(ch);
                let is_end_punct = ch == '.' || ch == '!' || ch == '?';
                let is_next_space = if i + 1 < len { chars[i + 1].1.is_whitespace() } else { true };
                if is_end_punct && is_next_space {
                    results.push(SegmentItem {
                        segment: current_segment,
                        index: current_index,
                        input: input.to_string(),
                        is_word_like: None,
                    });
                    current_segment = String::new();
                }
            }
            if !current_segment.is_empty() {
                results.push(SegmentItem {
                    segment: current_segment,
                    index: current_index,
                    input: input.to_string(),
                    is_word_like: None,
                });
            }
        }
        _ => {
            // "grapheme"
            for (idx, ch) in input.char_indices() {
                results.push(SegmentItem {
                    segment: ch.to_string(),
                    index: idx,
                    input: input.to_string(),
                    is_word_like: None,
                });
            }
        }
    }
    results
}

#[derive(Clone, Debug)]
pub struct ParsedLocale {
    pub base_name: String,
    pub language: String,
    pub script: Option<String>,
    pub region: Option<String>,
    pub calendar: Option<String>,
    pub collation: Option<String>,
    pub hour_cycle: Option<String>,
    pub numbering_system: Option<String>,
    pub numeric: Option<bool>,
    pub case_first: Option<String>,
    pub canonical_tag: String,
}

impl ParsedLocale {
    pub fn parse(tag: &str) -> Self {
        let clean = tag.trim().replace('_', "-");
        let parts: Vec<&str> = clean.split('-').collect();

        let mut language = "en".to_string();
        let mut script = None;
        let mut region = None;
        let mut calendar = None;
        let mut collation = None;
        let mut hour_cycle = None;
        let mut numbering_system = None;
        let mut numeric = None;
        let mut case_first = None;

        let mut i = 0;
        if i < parts.len() {
            language = parts[i].to_lowercase();
            i += 1;
        }

        // Script: 4 letters
        if i < parts.len() && parts[i].len() == 4 && parts[i].chars().all(|c| c.is_ascii_alphabetic()) {
            let mut chars = parts[i].chars();
            let first = chars.next().unwrap().to_ascii_uppercase();
            let rest: String = chars.map(|c| c.to_ascii_lowercase()).collect();
            script = Some(format!("{}{}", first, rest));
            i += 1;
        }

        // Region: 2 letters or 3 digits
        if i < parts.len() && ((parts[i].len() == 2 && parts[i].chars().all(|c| c.is_ascii_alphabetic())) || (parts[i].len() == 3 && parts[i].chars().all(|c| c.is_ascii_digit()))) {
            region = Some(parts[i].to_ascii_uppercase());
            i += 1;
        }

        // Unicode extensions -u-...
        while i < parts.len() {
            if parts[i] == "u" && i + 1 < parts.len() {
                i += 1;
                while i < parts.len() {
                    let key = parts[i];
                    if key.len() == 2 {
                        let val = if i + 1 < parts.len() && parts[i + 1].len() > 2 {
                            i += 1;
                            Some(parts[i].to_string())
                        } else if i + 1 < parts.len() && parts[i + 1].len() <= 2 && parts[i + 1] != "u" {
                            i += 1;
                            Some(parts[i].to_string())
                        } else {
                            None
                        };
                        match key {
                            "ca" => calendar = val,
                            "co" => collation = val,
                            "hc" => hour_cycle = val,
                            "nu" => numbering_system = val,
                            "kn" => numeric = val.map(|v| v == "true"),
                            "kf" => case_first = val,
                            _ => {}
                        }
                    } else if key == "kn" {
                        numeric = Some(true);
                    }
                    i += 1;
                }
                break;
            }
            i += 1;
        }

        let mut base_parts = vec![language.clone()];
        if let Some(ref s) = script {
            base_parts.push(s.clone());
        }
        if let Some(ref r) = region {
            base_parts.push(r.clone());
        }
        let base_name = base_parts.join("-");

        let mut tag_parts = vec![base_name.clone()];
        let mut u_exts = Vec::new();
        if let Some(ref ca) = calendar { u_exts.push(format!("ca-{}", ca)); }
        if let Some(ref co) = collation { u_exts.push(format!("co-{}", co)); }
        if let Some(ref hc) = hour_cycle { u_exts.push(format!("hc-{}", hc)); }
        if let Some(ref kf) = case_first { u_exts.push(format!("kf-{}", kf)); }
        if let Some(kn) = numeric { if kn { u_exts.push("kn".to_string()); } }
        if let Some(ref nu) = numbering_system { u_exts.push(format!("nu-{}", nu)); }

        if !u_exts.is_empty() {
            tag_parts.push("u".to_string());
            tag_parts.extend(u_exts);
        }
        let canonical_tag = tag_parts.join("-");

        Self {
            base_name,
            language,
            script,
            region,
            calendar,
            collation,
            hour_cycle,
            numbering_system,
            numeric,
            case_first,
            canonical_tag,
        }
    }

    pub fn maximize(&self) -> String {
        let (script, region) = match self.language.as_str() {
            "en" => ("Latn", "US"),
            "zh" => ("Hans", "CN"),
            "ja" => ("Jpan", "JP"),
            "fr" => ("Latn", "FR"),
            "de" => ("Latn", "DE"),
            "es" => ("Latn", "ES"),
            "ru" => ("Cyrl", "RU"),
            "ar" => ("Arab", "EG"),
            "ko" => ("Kore", "KR"),
            _ => ("Latn", "US"),
        };
        let s = self.script.as_deref().unwrap_or(script);
        let r = self.region.as_deref().unwrap_or(region);
        format!("{}-{}-{}", self.language, s, r)
    }

    pub fn minimize(&self) -> String {
        self.language.clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct DurationRecord {
    pub years: i64,
    pub months: i64,
    pub weeks: i64,
    pub days: i64,
    pub hours: i64,
    pub minutes: i64,
    pub seconds: i64,
    pub milliseconds: i64,
    pub microseconds: i64,
    pub nanoseconds: i64,
}

impl DurationRecord {
    pub fn from_js(val: &JSValue) -> Self {
        let mut rec = Self::default();
        if let JSValue::Object(o) = val {
            let b = o.borrow();
            rec.years = b.get_property("years").to_number() as i64;
            rec.months = b.get_property("months").to_number() as i64;
            rec.weeks = b.get_property("weeks").to_number() as i64;
            rec.days = b.get_property("days").to_number() as i64;
            rec.hours = b.get_property("hours").to_number() as i64;
            rec.minutes = b.get_property("minutes").to_number() as i64;
            rec.seconds = b.get_property("seconds").to_number() as i64;
            rec.milliseconds = b.get_property("milliseconds").to_number() as i64;
            rec.microseconds = b.get_property("microseconds").to_number() as i64;
            rec.nanoseconds = b.get_property("nanoseconds").to_number() as i64;
        }
        rec
    }
}

fn format_duration(style: &str, record: &DurationRecord) -> String {
    if style == "digital" {
        return format!("{}:{:02}:{:02}", record.hours, record.minutes, record.seconds);
    }
    let mut parts = Vec::new();
    if record.years > 0 {
        let unit = if style == "long" { if record.years == 1 { "year" } else { "years" } } else if style == "short" { "yr" } else { "y" };
        if style == "narrow" { parts.push(format!("{}{}", record.years, unit)); }
        else { parts.push(format!("{} {}", record.years, unit)); }
    }
    if record.months > 0 {
        let unit = if style == "long" { if record.months == 1 { "month" } else { "months" } } else if style == "short" { "mth" } else { "m" };
        if style == "narrow" { parts.push(format!("{}{}", record.months, unit)); }
        else { parts.push(format!("{} {}", record.months, unit)); }
    }
    if record.weeks > 0 {
        let unit = if style == "long" { if record.weeks == 1 { "week" } else { "weeks" } } else if style == "short" { "wk" } else { "w" };
        if style == "narrow" { parts.push(format!("{}{}", record.weeks, unit)); }
        else { parts.push(format!("{} {}", record.weeks, unit)); }
    }
    if record.days > 0 {
        let unit = if style == "long" { if record.days == 1 { "day" } else { "days" } } else if style == "short" { "day" } else { "d" };
        if style == "narrow" { parts.push(format!("{}{}", record.days, unit)); }
        else { parts.push(format!("{} {}", record.days, unit)); }
    }
    if record.hours > 0 {
        let unit = if style == "long" { if record.hours == 1 { "hour" } else { "hours" } } else if style == "short" { "hr" } else { "h" };
        if style == "narrow" { parts.push(format!("{}{}", record.hours, unit)); }
        else { parts.push(format!("{} {}", record.hours, unit)); }
    }
    if record.minutes > 0 {
        let unit = if style == "long" { if record.minutes == 1 { "minute" } else { "minutes" } } else if style == "short" { "min" } else { "m" };
        if style == "narrow" { parts.push(format!("{}{}", record.minutes, unit)); }
        else { parts.push(format!("{} {}", record.minutes, unit)); }
    }
    if record.seconds > 0 || parts.is_empty() {
        let unit = if style == "long" { if record.seconds == 1 { "second" } else { "seconds" } } else if style == "short" { "sec" } else { "s" };
        if style == "narrow" { parts.push(format!("{}{}", record.seconds, unit)); }
        else { parts.push(format!("{} {}", record.seconds, unit)); }
    }
    if style == "narrow" {
        parts.join(" ")
    } else {
        parts.join(", ")
    }
}

fn format_duration_to_parts(style: &str, record: &DurationRecord) -> Vec<JSValue> {
    let mut parts = Vec::new();
    let add_unit = |parts: &mut Vec<JSValue>, val: i64, unit_singular: &str, unit_plural: &str, unit_short: &str, unit_narrow: &str| {
        let int_p = JSObject::new_empty(None);
        JSObject::set_property(&int_p, "type", JSValue::String("integer".to_string()));
        JSObject::set_property(&int_p, "value", JSValue::String(val.to_string()));
        JSObject::set_property(&int_p, "unit", JSValue::String(unit_singular.to_string()));
        parts.push(JSValue::Object(int_p));

        let lit_val = match style {
            "long" => format!(" {}", if val == 1 { unit_singular } else { unit_plural }),
            "short" => format!(" {}", unit_short),
            "narrow" => unit_narrow.to_string(),
            _ => format!(" {}", unit_short),
        };
        let lit_p = JSObject::new_empty(None);
        JSObject::set_property(&lit_p, "type", JSValue::String("literal".to_string()));
        JSObject::set_property(&lit_p, "value", JSValue::String(lit_val));
        parts.push(JSValue::Object(lit_p));
    };

    let mut added_any = false;
    if record.years > 0 {
        add_unit(&mut parts, record.years, "year", "years", "yr", "y");
        added_any = true;
    }
    if record.months > 0 {
        if added_any && style != "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(", ".to_string()));
            parts.push(JSValue::Object(sep));
        } else if added_any && style == "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(" ".to_string()));
            parts.push(JSValue::Object(sep));
        }
        add_unit(&mut parts, record.months, "month", "months", "mth", "m");
        added_any = true;
    }
    if record.weeks > 0 {
        if added_any && style != "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(", ".to_string()));
            parts.push(JSValue::Object(sep));
        } else if added_any && style == "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(" ".to_string()));
            parts.push(JSValue::Object(sep));
        }
        add_unit(&mut parts, record.weeks, "week", "weeks", "wk", "w");
        added_any = true;
    }
    if record.days > 0 {
        if added_any && style != "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(", ".to_string()));
            parts.push(JSValue::Object(sep));
        } else if added_any && style == "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(" ".to_string()));
            parts.push(JSValue::Object(sep));
        }
        add_unit(&mut parts, record.days, "day", "days", "day", "d");
        added_any = true;
    }
    if record.hours > 0 {
        if added_any && style != "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(", ".to_string()));
            parts.push(JSValue::Object(sep));
        } else if added_any && style == "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(" ".to_string()));
            parts.push(JSValue::Object(sep));
        }
        add_unit(&mut parts, record.hours, "hour", "hours", "hr", "h");
        added_any = true;
    }
    if record.minutes > 0 {
        if added_any && style != "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(", ".to_string()));
            parts.push(JSValue::Object(sep));
        } else if added_any && style == "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(" ".to_string()));
            parts.push(JSValue::Object(sep));
        }
        add_unit(&mut parts, record.minutes, "minute", "minutes", "min", "m");
        added_any = true;
    }
    if record.seconds > 0 || !added_any {
        if added_any && style != "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(", ".to_string()));
            parts.push(JSValue::Object(sep));
        } else if added_any && style == "narrow" {
            let sep = JSObject::new_empty(None);
            JSObject::set_property(&sep, "type", JSValue::String("literal".to_string()));
            JSObject::set_property(&sep, "value", JSValue::String(" ".to_string()));
            parts.push(JSValue::Object(sep));
        }
        add_unit(&mut parts, record.seconds, "second", "seconds", "sec", "s");
    }
    parts
}
