//! Daily usage stats (Settings → Usage): tokens per day and what they cost.
//!
//! Every reply and every extra model call (auto mode's checks, compaction…) is a
//! row in the usage ledger with its input (incl. the part served from the
//! provider's cache), output and model. Cost comes from the provider's price
//! list; DeepSeek's (api-docs.deepseek.com/quick_start/pricing, Nov 2026) is
//! built in, with its peak / off-peak hours. Rows from before the split was
//! recorded don't say how much input was cached: those are priced at the cache
//! rate measured on newer rows and marked as estimated.

use chrono::{Datelike, Duration, Local, TimeZone, Timelike, Utc};
use serde::Serialize;

use iris_core::UsageRow;

use crate::api::AppState;

/// USD per 1M tokens at peak hours: input served from the cache, other input, output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub cached: f64,
    pub input: f64,
    pub output: f64,
}

/// The built-in price list (peak; off-peak is half).
pub fn price_for(model: &str) -> Option<Price> {
    let m = model.to_ascii_lowercase();
    if m.contains("deepseek-v4-pro") {
        Some(Price { cached: 0.044, input: 1.32, output: 3.96 })
    } else if m.contains("deepseek-flash") || m.contains("deepseek-v4-flash") {
        Some(Price { cached: 0.006, input: 0.30, output: 1.20 })
    } else {
        None
    }
}

/// DeepSeek's peak hours: 01:00–04:00 and 06:00–10:00 UTC, Monday to Friday.
pub fn is_peak(at_ms: i64) -> bool {
    let Some(t) = Utc.timestamp_millis_opt(at_ms).single() else { return true };
    let weekday = t.weekday().num_days_from_monday() < 5;
    let h = t.hour();
    weekday && ((1..4).contains(&h) || (6..10).contains(&h))
}

/// The cost of one row in USD, or None when its model has no known price.
/// `cache_rate` stands in for rows that don't say how much input was cached.
pub fn cost_of(row: &UsageRow, default_model: &str, cache_rate: f64) -> Option<(f64, bool)> {
    let model = if row.model.is_empty() { default_model } else { &row.model };
    let price = price_for(model)?;
    // Very old rows only have a total: count it all as input.
    let (input, output) = if row.input == 0 && row.output == 0 { (row.tokens, 0) } else { (row.input, row.output) };
    let estimated = row.cached < 0;
    let cached = if estimated { (input as f64 * cache_rate).round() as i64 } else { row.cached.min(input) };
    let factor = if is_peak(row.created_at) { 1.0 } else { 0.5 };
    let usd = (price.cached * cached as f64 + price.input * (input - cached) as f64 + price.output * output as f64) / 1e6 * factor;
    Some((usd, estimated))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Day {
    /// Local date, YYYY-MM-DD.
    pub day: String,
    pub tokens: i64,
    pub input: i64,
    /// Known cached input (rows that report it).
    pub cached: i64,
    pub output: i64,
    pub cost: f64,
    /// Part of the cost is estimated (rows without a known cache split).
    pub estimated: bool,
    /// Tokens on models without a known price (not in `cost`).
    pub unpriced: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Daily {
    pub days: Vec<Day>,
    pub currency: &'static str,
    /// Share of input served from the cache, on rows that report it (0..1).
    pub cache_rate: f64,
    /// The prices used (per 1M tokens, peak; off-peak is half), for the default model.
    pub prices: Option<serde_json::Value>,
}

/// The cache rate measured on rows that report it (DeepSeek's is high, ~90%).
fn measured_cache_rate(rows: &[UsageRow]) -> f64 {
    let (hit, input) = rows.iter().filter(|r| r.cached >= 0 && r.input > 0).fold((0i64, 0i64), |(h, i), r| (h + r.cached, i + r.input));
    if input > 50_000 { hit as f64 / input as f64 } else { 0.9 }
}

/// Totals per local day for the last `days` days (today included), oldest first.
pub fn daily(rows: &[UsageRow], days: u32, default_model: &str, now: chrono::DateTime<Local>) -> Daily {
    let cache_rate = measured_cache_rate(rows);
    let today = now.date_naive();
    let mut out: Vec<Day> = (0..days)
        .rev()
        .map(|back| Day {
            day: (today - Duration::days(i64::from(back))).format("%Y-%m-%d").to_string(),
            tokens: 0,
            input: 0,
            cached: 0,
            output: 0,
            cost: 0.0,
            estimated: false,
            unpriced: 0,
        })
        .collect();
    for row in rows {
        let Some(at) = Local.timestamp_millis_opt(row.created_at).single() else { continue };
        let key = at.date_naive().format("%Y-%m-%d").to_string();
        let Some(day) = out.iter_mut().find(|d| d.day == key) else { continue };
        day.tokens += row.tokens;
        day.input += if row.input == 0 && row.output == 0 { row.tokens } else { row.input };
        day.cached += row.cached.max(0);
        day.output += row.output;
        match cost_of(row, default_model, cache_rate) {
            Some((usd, est)) => {
                day.cost += usd;
                day.estimated |= est;
            }
            None => day.unpriced += row.tokens,
        }
    }
    let prices = price_for(default_model).map(|p| serde_json::json!({ "model": default_model, "cached": p.cached, "input": p.input, "output": p.output }));
    Daily { days: out, currency: "USD", cache_rate, prices }
}

/// The ledger rows for the last `days` local days.
pub fn load(st: &AppState, days: u32) -> Daily {
    let now = Local::now();
    let start = now.date_naive() - Duration::days(i64::from(days.saturating_sub(1)));
    let since = start
        .and_hms_opt(0, 0, 0)
        .and_then(|m| Local.from_local_datetime(&m).earliest())
        .map_or(0, |d| d.timestamp_millis());
    let rows = st.store.usage_since(since).unwrap_or_default();
    let default_model = st.defaults.read().unwrap().model.clone().unwrap_or_default();
    daily(&rows, days, &default_model, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(at: &str, input: i64, cached: i64, output: i64, model: &str) -> UsageRow {
        let t = chrono::DateTime::parse_from_rfc3339(at).unwrap().timestamp_millis();
        UsageRow { created_at: t, tokens: input + output, input, cached, output, model: model.into() }
    }

    #[test]
    fn deepseek_prices_and_peak_hours() {
        assert_eq!(price_for("deepseek-flash").unwrap().input, 0.30);
        assert_eq!(price_for("deepseek-v4-pro").unwrap().output, 3.96);
        assert!(price_for("llama-3.3-70b").is_none());
        // Wednesday 07:30 UTC is peak; 05:00, 12:00 and Saturdays aren't.
        assert!(is_peak(row("2026-11-25T07:30:00Z", 0, 0, 0, "").created_at));
        assert!(!is_peak(row("2026-11-25T05:00:00Z", 0, 0, 0, "").created_at));
        assert!(!is_peak(row("2026-11-25T12:00:00Z", 0, 0, 0, "").created_at));
        assert!(!is_peak(row("2026-11-28T07:30:00Z", 0, 0, 0, "").created_at));
    }

    #[test]
    fn a_reply_costs_cached_plus_new_input_plus_output() {
        // 1M input of which 900k cached, 10k output, at peak on Flash:
        // 0.9 × $0.006 + 0.1 × $0.30 + 0.01 × $1.20 = $0.0474.
        let r = row("2026-11-25T07:30:00Z", 1_000_000, 900_000, 10_000, "deepseek-flash");
        let (usd, est) = cost_of(&r, "", 0.9).unwrap();
        assert!((usd - 0.0474).abs() < 1e-9, "{usd}");
        assert!(!est);
        // Off-peak is half.
        let r = row("2026-11-25T12:00:00Z", 1_000_000, 900_000, 10_000, "deepseek-flash");
        assert!((cost_of(&r, "", 0.9).unwrap().0 - 0.0237).abs() < 1e-9);
        // Unknown cache split: estimated with the given rate; empty model = the default.
        let r = row("2026-11-25T12:00:00Z", 1_000_000, -1, 10_000, "");
        let (usd, est) = cost_of(&r, "deepseek-flash", 0.9).unwrap();
        assert!(est && (usd - 0.0237).abs() < 1e-9);
        assert!(cost_of(&row("2026-11-25T12:00:00Z", 10, 0, 1, "llama"), "", 0.9).is_none());
    }

    #[test]
    fn days_are_filled_and_summed() {
        let now = Local.with_ymd_and_hms(2026, 11, 25, 15, 0, 0).unwrap();
        let today = now.with_hour(9).unwrap().timestamp_millis();
        let rows = vec![
            UsageRow { created_at: today, tokens: 1100, input: 1000, cached: 800, output: 100, model: "deepseek-flash".into() },
            UsageRow { created_at: today, tokens: 50, input: 50, cached: -1, output: 0, model: "mystery".into() },
        ];
        let d = daily(&rows, 7, "deepseek-flash", now);
        assert_eq!(d.days.len(), 7);
        assert_eq!(d.days.last().unwrap().day, "2026-11-25");
        let t = d.days.last().unwrap();
        assert_eq!((t.tokens, t.input, t.cached, t.output, t.unpriced), (1150, 1050, 800, 100, 50));
        assert!(t.cost > 0.0 && !t.estimated);
        assert_eq!(d.days[0].tokens, 0);
    }
}
