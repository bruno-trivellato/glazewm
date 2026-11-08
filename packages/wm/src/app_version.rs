use chrono::{DateTime, Datelike, Local, TimeDelta, TimeZone, Utc, Weekday};

const VERSION_NAME: &str = env!("VERSION_NUMBER");
const VERSION_CODE: &str = env!("VERSION_CODE");
const VERSION_DATE: &str = env!("VERSION_DATE");

pub fn version_with_age(now: DateTime<Utc>) -> String {
  match version_bumped_at() {
    Some(bumped_at) => format!(
      "{} · {}",
      version_name_and_code(),
      age_since(bumped_at, now, &Local)
    ),
    None => version_name_and_code(),
  }
}

pub fn version_with_full_date() -> String {
  match version_bumped_at() {
    Some(bumped_at) => format!(
      "{} · {}",
      version_name_and_code(),
      full_date_in_portuguese(&bumped_at.with_timezone(&Local))
    ),
    None => version_name_and_code(),
  }
}

fn version_name_and_code() -> String {
  format!("v{VERSION_NAME} ({VERSION_CODE})")
}

fn version_bumped_at() -> Option<DateTime<Utc>> {
  DateTime::parse_from_rfc3339(VERSION_DATE)
    .ok()
    .map(|bumped_at| bumped_at.with_timezone(&Utc))
}

fn age_since<Tz: TimeZone>(
  bumped_at: DateTime<Utc>,
  now: DateTime<Utc>,
  display_timezone: &Tz,
) -> String
where
  Tz::Offset: std::fmt::Display,
{
  let age_ignoring_clock_skew = (now - bumped_at).max(TimeDelta::zero());

  if age_ignoring_clock_skew < TimeDelta::minutes(1) {
    format!("há {}s", age_ignoring_clock_skew.num_seconds())
  } else if age_ignoring_clock_skew < TimeDelta::hours(1) {
    format!("há {}min", age_ignoring_clock_skew.num_minutes())
  } else if age_ignoring_clock_skew < TimeDelta::days(1) {
    format!("há {}h", age_ignoring_clock_skew.num_hours())
  } else if age_ignoring_clock_skew < TimeDelta::days(2) {
    "há 1 dia".to_string()
  } else if age_ignoring_clock_skew < TimeDelta::days(7) {
    weekday_in_portuguese(
      bumped_at.with_timezone(display_timezone).weekday(),
    )
    .to_string()
  } else {
    bumped_at
      .with_timezone(display_timezone)
      .format("%d/%m/%y")
      .to_string()
  }
}

fn full_date_in_portuguese<Tz: TimeZone>(date: &DateTime<Tz>) -> String
where
  Tz::Offset: std::fmt::Display,
{
  format!(
    "{} de {} de {}, {}",
    date.day(),
    month_in_portuguese(date.month()),
    date.year(),
    date.format("%H:%M")
  )
}

fn weekday_in_portuguese(weekday: Weekday) -> &'static str {
  match weekday {
    Weekday::Mon => "segunda",
    Weekday::Tue => "terça",
    Weekday::Wed => "quarta",
    Weekday::Thu => "quinta",
    Weekday::Fri => "sexta",
    Weekday::Sat => "sábado",
    Weekday::Sun => "domingo",
  }
}

fn month_in_portuguese(month: u32) -> &'static str {
  const MONTHS: [&str; 12] = [
    "janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho",
    "agosto", "setembro", "outubro", "novembro", "dezembro",
  ];

  usize::try_from(month)
    .ok()
    .and_then(|month| MONTHS.get(month.wrapping_sub(1)))
    .copied()
    .unwrap_or("")
}

#[cfg(test)]
mod tests {
  use super::*;

  fn utc(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
  }

  #[test]
  fn age_follows_the_shrinking_scale() {
    let bumped_at = utc("2026-08-11T07:24:00Z");

    let age_at = |now: &str| age_since(bumped_at, utc(now), &Utc);

    assert_eq!(age_at("2026-08-11T07:24:45Z"), "há 45s");
    assert_eq!(age_at("2026-08-11T07:36:00Z"), "há 12min");
    assert_eq!(age_at("2026-08-11T10:24:00Z"), "há 3h");
    assert_eq!(age_at("2026-08-12T09:00:00Z"), "há 1 dia");
    assert_eq!(age_at("2026-08-14T09:00:00Z"), "terça");
    assert_eq!(age_at("2026-08-20T09:00:00Z"), "11/08/26");
    assert_eq!(age_at("2026-08-11T07:00:00Z"), "há 0s");
  }

  #[test]
  fn full_date_is_written_in_portuguese() {
    assert_eq!(
      full_date_in_portuguese(&utc("2026-08-11T07:24:00Z")),
      "11 de agosto de 2026, 07:24"
    );
  }
}
