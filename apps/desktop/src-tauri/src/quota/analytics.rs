use super::{ProviderQuotaWindow, QuotaWindowKind};
use chrono::{DateTime, TimeDelta, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const RECENT_WINDOW_SECONDS: i64 = 60 * 60;
const MAX_RECENT_EXPANSION_SECONDS: i64 = 6 * 60 * 60;
const MIN_OBSERVATION_SECONDS: i64 = 10 * 60;
const MIN_SAMPLES: usize = 3;
const MINOR_JITTER_BPS: i64 = 50;
const MAJOR_DROP_BPS: i64 = 500;
const STALE_AFTER_SECONDS: i64 = 15 * 60;
const MAX_HISTORY_ROWS: i64 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaAnalysisConfidence {
    Insufficient,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaAnalysisStatus {
    Gathering,
    NoRecentChange,
    Active,
    LimitReached,
    Informational,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindowAnalysis {
    pub provider: String,
    pub window_key: String,
    pub label: String,
    pub kind: QuotaWindowKind,
    pub cap_bearing: bool,
    pub utilization_bps: i64,
    pub remaining_bps: i64,
    pub period_starts_at: Option<DateTime<Utc>>,
    pub resets_at: Option<DateTime<Utc>>,
    pub observed_at: DateTime<Utc>,
    pub recent_burn_bps_per_hour: Option<i64>,
    pub period_average_burn_bps_per_hour: Option<i64>,
    pub projected_exhaustion_at: Option<DateTime<Utc>>,
    pub projected_before_reset: Option<bool>,
    pub sample_count: usize,
    pub observation_span_seconds: i64,
    pub confidence: QuotaAnalysisConfidence,
    pub status: QuotaAnalysisStatus,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Sample {
    observed_at: DateTime<Utc>,
    utilization_bps: i64,
}

pub(super) fn analyze_windows(
    connection: &Connection,
    provider: &str,
    windows: &[ProviderQuotaWindow],
    observed_at: DateTime<Utc>,
    source_device_id: &str,
    refresh_stale: bool,
) -> Result<Vec<QuotaWindowAnalysis>, rusqlite::Error> {
    windows
        .iter()
        .map(|window| {
            let samples =
                load_samples(connection, provider, window, observed_at, source_device_id)?;
            Ok(analyze_series(
                provider,
                window,
                observed_at,
                samples,
                refresh_stale,
                Utc::now(),
            ))
        })
        .collect()
}

fn load_samples(
    connection: &Connection,
    provider: &str,
    window: &ProviderQuotaWindow,
    observed_at: DateTime<Utc>,
    source_device_id: &str,
) -> Result<Vec<Sample>, rusqlite::Error> {
    let period_start = window.period_starts_at.map(|value| value.to_rfc3339());
    let reset = window.resets_at.map(|value| value.to_rfc3339());
    let history_start = window
        .period_starts_at
        .unwrap_or(observed_at - TimeDelta::seconds(MAX_RECENT_EXPANSION_SECONDS));
    let mut statement = connection.prepare(
        "SELECT utilization_bps, observed_at
         FROM provider_quota_snapshots
         WHERE provider = ?1 AND window_key = ?2 AND source_device_id = ?3
           AND observed_at BETWEEN ?4 AND ?5
           AND ((?6 IS NULL AND period_starts_at IS NULL) OR period_starts_at = ?6)
           AND ((?7 IS NULL AND resets_at IS NULL) OR resets_at = ?7)
         ORDER BY observed_at DESC, updated_at DESC
         LIMIT ?8",
    )?;
    let rows = statement.query_map(
        params![
            provider,
            window.key,
            source_device_id,
            history_start.to_rfc3339(),
            observed_at.to_rfc3339(),
            period_start,
            reset,
            MAX_HISTORY_ROWS,
        ],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut samples = rows
        .filter_map(Result::ok)
        .filter_map(|(utilization_bps, observed)| {
            DateTime::parse_from_rfc3339(&observed)
                .ok()
                .map(|value| Sample {
                    observed_at: value.with_timezone(&Utc),
                    utilization_bps,
                })
        })
        .collect::<Vec<_>>();
    samples.sort_by_key(|sample| sample.observed_at);
    samples.dedup_by_key(|sample| sample.observed_at);
    Ok(samples)
}

fn analyze_series(
    provider: &str,
    window: &ProviderQuotaWindow,
    observed_at: DateTime<Utc>,
    mut samples: Vec<Sample>,
    refresh_stale: bool,
    now: DateTime<Utc>,
) -> QuotaWindowAnalysis {
    samples.sort_by_key(|sample| sample.observed_at);
    samples.dedup_by_key(|sample| sample.observed_at);

    // A large unexplained drop is a period boundary even when provider metadata failed
    // to change. Only the segment containing the current reading may affect its pace.
    let segment_start = samples
        .windows(2)
        .rposition(|pair| pair[0].utilization_bps - pair[1].utilization_bps > MAJOR_DROP_BPS)
        .map_or(0, |index| index + 1);
    let segment = &samples[segment_start..];
    let recent_cutoff = observed_at - TimeDelta::seconds(RECENT_WINDOW_SECONDS);
    let expanded_cutoff = observed_at - TimeDelta::seconds(MAX_RECENT_EXPANSION_SECONDS);
    let recent = segment
        .iter()
        .filter(|sample| sample.observed_at >= recent_cutoff)
        .cloned()
        .collect::<Vec<_>>();
    let recent_span = observation_span(&recent);
    let selected = if recent.len() >= MIN_SAMPLES && recent_span >= MIN_OBSERVATION_SECONDS {
        recent
    } else {
        segment
            .iter()
            .filter(|sample| sample.observed_at >= expanded_cutoff)
            .cloned()
            .collect()
    };
    let sample_count = selected.len();
    let observation_span_seconds = observation_span(&selected);
    let enough_history =
        sample_count >= MIN_SAMPLES && observation_span_seconds >= MIN_OBSERVATION_SECONDS;
    let stale = refresh_stale || (now - observed_at).num_seconds() > STALE_AFTER_SECONDS;
    let cap_bearing = matches!(
        window.kind,
        QuotaWindowKind::Rolling
            | QuotaWindowKind::Weekly
            | QuotaWindowKind::Monthly
            | QuotaWindowKind::ModelWeekly
    );
    let monotonic_consistency = monotonic_consistency(&selected);
    let confidence = if !enough_history {
        QuotaAnalysisConfidence::Insufficient
    } else if stale || sample_count < 4 || observation_span_seconds < 30 * 60 {
        QuotaAnalysisConfidence::Low
    } else if sample_count >= 7
        && observation_span_seconds >= 60 * 60
        && monotonic_consistency >= 80
    {
        QuotaAnalysisConfidence::High
    } else {
        QuotaAnalysisConfidence::Medium
    };
    let recent_burn = enough_history.then(|| theil_sen_bps_per_hour(&selected));
    let utilization_bps = window.utilization_bps.clamp(0, 10_000);
    let remaining_bps = (10_000 - utilization_bps).max(0);
    let period_average_burn_bps_per_hour = if cap_bearing && enough_history {
        window.period_starts_at.and_then(|period_start| {
            let elapsed = (observed_at - period_start).num_seconds();
            (elapsed >= MIN_OBSERVATION_SECONDS)
                .then(|| utilization_bps.saturating_mul(3_600) / elapsed)
        })
    } else {
        None
    };
    let may_project = cap_bearing
        && !stale
        && confidence >= QuotaAnalysisConfidence::Medium
        && utilization_bps < 10_000;
    let projected_exhaustion_at = recent_burn
        .filter(|rate| may_project && *rate > 0)
        .and_then(|rate| {
            let seconds = remaining_bps.saturating_mul(3_600) / rate;
            let horizon = match window.kind {
                QuotaWindowKind::Rolling => 24 * 60 * 60,
                QuotaWindowKind::Weekly | QuotaWindowKind::ModelWeekly => 8 * 24 * 60 * 60,
                QuotaWindowKind::Monthly => 32 * 24 * 60 * 60,
                QuotaWindowKind::Product | QuotaWindowKind::Other => 0,
            };
            (seconds > 0 && (window.resets_at.is_some() || seconds <= horizon))
                .then(|| observed_at + TimeDelta::seconds(seconds))
        });
    let projected_before_reset = projected_exhaustion_at
        .zip(window.resets_at)
        .map(|(projected, reset)| projected < reset);
    let status = if !cap_bearing {
        QuotaAnalysisStatus::Informational
    } else if utilization_bps >= 10_000 {
        QuotaAnalysisStatus::LimitReached
    } else if !enough_history {
        QuotaAnalysisStatus::Gathering
    } else if stale {
        QuotaAnalysisStatus::Stale
    } else if recent_burn.unwrap_or_default() <= 0 {
        QuotaAnalysisStatus::NoRecentChange
    } else {
        QuotaAnalysisStatus::Active
    };

    QuotaWindowAnalysis {
        provider: provider.to_owned(),
        window_key: window.key.clone(),
        label: window.label.clone(),
        kind: window.kind,
        cap_bearing,
        utilization_bps,
        remaining_bps,
        period_starts_at: window.period_starts_at,
        resets_at: window.resets_at,
        observed_at,
        recent_burn_bps_per_hour: recent_burn,
        period_average_burn_bps_per_hour,
        projected_exhaustion_at,
        projected_before_reset,
        sample_count,
        observation_span_seconds,
        confidence,
        status,
        stale,
    }
}

fn observation_span(samples: &[Sample]) -> i64 {
    samples
        .first()
        .zip(samples.last())
        .map_or(0, |(first, last)| {
            (last.observed_at - first.observed_at).num_seconds()
        })
}

fn monotonic_consistency(samples: &[Sample]) -> i64 {
    let pairs = samples.windows(2).count() as i64;
    if pairs == 0 {
        return 0;
    }
    let consistent = samples
        .windows(2)
        .filter(|pair| pair[1].utilization_bps + MINOR_JITTER_BPS >= pair[0].utilization_bps)
        .count() as i64;
    consistent * 100 / pairs
}

fn theil_sen_bps_per_hour(samples: &[Sample]) -> i64 {
    let mut adjusted = Vec::with_capacity(samples.len());
    let mut high_water = 0;
    for sample in samples {
        // Quota never regenerates within an active period. Neutralizing downward
        // corrections keeps provider jitter from becoming a negative burn rate.
        high_water = high_water.max(sample.utilization_bps);
        adjusted.push((sample.observed_at, high_water));
    }
    let mut slopes = Vec::new();
    for (left_index, (left_time, left_value)) in adjusted.iter().enumerate() {
        for (right_time, right_value) in adjusted.iter().skip(left_index + 1) {
            let seconds = (*right_time - *left_time).num_seconds();
            if seconds > 0 {
                slopes.push((right_value - left_value).saturating_mul(3_600) / seconds);
            }
        }
    }
    if slopes.is_empty() {
        return 0;
    }
    slopes.sort_unstable();
    let middle = slopes.len() / 2;
    if slopes.len() % 2 == 0 {
        (slopes[middle - 1] + slopes[middle]) / 2
    } else {
        slopes[middle]
    }
    .max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn at(minutes: i64) -> DateTime<Utc> {
        "2026-09-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap() + TimeDelta::minutes(minutes)
    }

    fn sample(minutes: i64, bps: i64) -> Sample {
        Sample {
            observed_at: at(minutes),
            utilization_bps: bps,
        }
    }

    fn window(kind: QuotaWindowKind, utilization_bps: i64) -> ProviderQuotaWindow {
        ProviderQuotaWindow {
            key: "window".into(),
            label: "Window".into(),
            kind,
            scope: None,
            utilization_bps,
            period_starts_at: Some(at(-60)),
            resets_at: Some(at(180)),
        }
    }

    fn analyze(kind: QuotaWindowKind, values: &[(i64, i64)]) -> QuotaWindowAnalysis {
        let samples = values
            .iter()
            .map(|(minutes, bps)| sample(*minutes, *bps))
            .collect::<Vec<_>>();
        let observed_at = samples.last().unwrap().observed_at;
        analyze_series(
            "test",
            &window(kind, samples.last().unwrap().utilization_bps),
            observed_at,
            samples,
            false,
            observed_at,
        )
    }

    #[test]
    fn robust_rate_uses_time_and_exact_basis_points() {
        let result = analyze(
            QuotaWindowKind::Rolling,
            &[(0, 1_000), (20, 2_000), (60, 4_000)],
        );
        assert_eq!(result.recent_burn_bps_per_hour, Some(3_000));
        assert_eq!(result.remaining_bps, 6_000);
        assert_eq!(result.confidence, QuotaAnalysisConfidence::Low);
        assert_eq!(result.projected_exhaustion_at, None);
    }

    #[test]
    fn flat_and_minor_jitter_never_create_negative_burn() {
        let flat = analyze(
            QuotaWindowKind::Weekly,
            &[(0, 4_000), (20, 4_000), (40, 4_000)],
        );
        assert_eq!(flat.status, QuotaAnalysisStatus::NoRecentChange);
        assert_eq!(flat.recent_burn_bps_per_hour, Some(0));
        let jitter = analyze(
            QuotaWindowKind::Weekly,
            &[(0, 4_000), (20, 4_020), (40, 4_010), (60, 4_050)],
        );
        assert!(jitter.recent_burn_bps_per_hour.unwrap() >= 0);
    }

    #[test]
    fn major_drop_starts_a_new_segment() {
        let result = analyze(
            QuotaWindowKind::Weekly,
            &[(0, 9_100), (15, 9_400), (30, 500), (45, 650), (60, 800)],
        );
        assert_eq!(result.sample_count, 3);
        assert_eq!(result.recent_burn_bps_per_hour, Some(600));
    }

    #[test]
    fn insufficient_history_has_no_rate_or_projection() {
        let result = analyze(QuotaWindowKind::Rolling, &[(0, 1_000), (5, 1_100)]);
        assert_eq!(result.confidence, QuotaAnalysisConfidence::Insufficient);
        assert_eq!(result.recent_burn_bps_per_hour, None);
        assert_eq!(result.projected_exhaustion_at, None);
    }

    #[test]
    fn fast_and_slow_burn_compare_projection_to_reset() {
        let fast = analyze(
            QuotaWindowKind::Rolling,
            &[(0, 7_000), (20, 7_667), (40, 8_333), (60, 9_000)],
        );
        assert_eq!(fast.projected_before_reset, Some(true));
        let mut slow_window = window(QuotaWindowKind::Weekly, 4_000);
        slow_window.resets_at = Some(at(90));
        let slow = analyze_series(
            "test",
            &slow_window,
            at(60),
            vec![
                sample(0, 3_000),
                sample(20, 3_333),
                sample(40, 3_667),
                sample(60, 4_000),
            ],
            false,
            at(60),
        );
        assert_eq!(slow.projected_before_reset, Some(false));
    }

    #[test]
    fn stale_and_at_limit_suppress_projection() {
        let samples = vec![sample(0, 8_000), sample(30, 9_000), sample(60, 9_900)];
        let stale = analyze_series(
            "test",
            &window(QuotaWindowKind::Rolling, 9_900),
            at(60),
            samples,
            true,
            at(60),
        );
        assert_eq!(stale.status, QuotaAnalysisStatus::Stale);
        assert_eq!(stale.projected_exhaustion_at, None);
        let reached = analyze(
            QuotaWindowKind::Rolling,
            &[(0, 9_000), (30, 9_500), (60, 10_000)],
        );
        assert_eq!(reached.status, QuotaAnalysisStatus::LimitReached);
        assert_eq!(reached.remaining_bps, 0);
    }

    #[test]
    fn product_is_informational_without_independent_eta() {
        let result = analyze(
            QuotaWindowKind::Product,
            &[(0, 1_000), (30, 2_000), (60, 3_000)],
        );
        assert!(!result.cap_bearing);
        assert_eq!(result.status, QuotaAnalysisStatus::Informational);
        assert_eq!(result.projected_exhaustion_at, None);
    }

    #[test]
    fn known_period_produces_period_average() {
        let result = analyze(
            QuotaWindowKind::Monthly,
            &[(0, 1_000), (30, 1_500), (60, 2_000)],
        );
        assert_eq!(result.period_average_burn_bps_per_hour, Some(1_000));
    }

    #[test]
    fn only_explicit_cap_kinds_receive_exhaustion_analysis() {
        for kind in [
            QuotaWindowKind::Rolling,
            QuotaWindowKind::Weekly,
            QuotaWindowKind::Monthly,
            QuotaWindowKind::ModelWeekly,
        ] {
            assert!(analyze(kind, &[(0, 1_000), (30, 2_000), (60, 3_000)]).cap_bearing);
        }
        for kind in [QuotaWindowKind::Product, QuotaWindowKind::Other] {
            let result = analyze(kind, &[(0, 1_000), (30, 2_000), (60, 3_000)]);
            assert!(!result.cap_bearing);
            assert_eq!(result.projected_exhaustion_at, None);
        }
    }

    #[test]
    fn history_query_never_crosses_reset_or_source_device() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE provider_quota_snapshots(
               provider TEXT, window_key TEXT, source_device_id TEXT,
               utilization_bps INTEGER, observed_at TEXT, updated_at TEXT,
               period_starts_at TEXT, resets_at TEXT
             );
             CREATE INDEX idx_quota_history
               ON provider_quota_snapshots(provider, window_key, observed_at DESC);",
            )
            .unwrap();
        let insert = |device: &str, minutes: i64, bps: i64, reset: &str| {
            let observed = at(minutes).to_rfc3339();
            connection
                .execute(
                    "INSERT INTO provider_quota_snapshots
                 VALUES('claude','window',?1,?2,?3,?3,NULL,?4)",
                    params![device, bps, observed, reset],
                )
                .unwrap();
        };
        insert("device-a", 0, 9_400, "2026-09-01T13:00:00+00:00");
        insert("device-a", 30, 500, "2026-09-01T15:00:00+00:00");
        insert("device-b", 40, 8_500, "2026-09-01T15:00:00+00:00");
        insert("device-a", 60, 800, "2026-09-01T15:00:00+00:00");
        let mut current = window(QuotaWindowKind::Rolling, 800);
        current.period_starts_at = None;
        current.resets_at = Some("2026-09-01T15:00:00Z".parse().unwrap());
        let samples = load_samples(&connection, "claude", &current, at(60), "device-a").unwrap();
        assert_eq!(
            samples
                .iter()
                .map(|item| item.utilization_bps)
                .collect::<Vec<_>>(),
            vec![500, 800]
        );
    }
}
