use anyhow::Result;
use chrono::{DateTime, Local, NaiveTime, Utc};
use directories_next::BaseDirs;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocationInfo {
    sunset: NaiveTime,
    sunrise: NaiveTime,
}

fn get_cache_path() -> PathBuf {
    let mut cache = BaseDirs::new().unwrap().cache_dir().to_owned();
    cache.push("themer-location");
    cache
}

impl LocationInfo {
    pub fn get_cached() -> Option<LocationInfo> {
        if let Ok(content) = std::fs::read_to_string(get_cache_path()) {
            let times = content
                .split(",")
                .map(NaiveTime::from_str)
                .collect::<Result<Vec<NaiveTime>, _>>()
                .ok()?;

            match times.as_slice() {
                [sunset, sunrise] => Some(LocationInfo {
                    sunset: *sunset,
                    sunrise: *sunrise,
                }),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn cache(&self) -> Result<()> {
        std::fs::write(
            get_cache_path(),
            format!("{},{}", self.sunset, self.sunrise),
        )?;

        Ok(())
    }
    pub fn get_theme(&self) -> Theme {
        let now = Local::now().time();

        if now > self.sunset || now < self.sunrise {
            Theme::Night
        } else {
            Theme::Day
        }
    }

    fn estimate() -> Result<Self> {
        #[derive(serde::Deserialize)]
        struct IpInfo {
            #[serde(rename = "latitude")]
            lat: f64,
            #[serde(rename = "longitude")]
            lon: f64,
        }

        let IpInfo { lat, lon } = minreq::get("https://freegeoip.app/json/")
            .send()?
            .json()
            .map_err(|e| {
                log::error!("Bad response from IP API: {}", e);
                e
            })?;

        let (sunset, sunrise) = match spa::calc_sunrise_and_set(Utc::now(), lat, lon)? {
            spa::SunriseAndSet::Daylight(set, rise) => {
                let rise: DateTime<Local> = rise.into();
                let set: DateTime<Local> = set.into();
                (rise.time(), set.time())
            }
            _ => (
                NaiveTime::from_hms(23, 59, 59),
                NaiveTime::from_hms(0, 0, 0),
            ),
        };
        Self { sunset, sunrise }.cache()?;

        Ok(Self { sunset, sunrise })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Theme {
    Night,
    Day,
}

impl Theme {
    fn set(&self) -> Result<()> {
        std::process::Command::new("systemctl")
            .args(&[
                "--user",
                "set-environment",
                &format!(
                    "THEME={}",
                    match self {
                        Self::Night => "dark",
                        _ => "light",
                    }
                ),
            ])
            .spawn()?;

        std::process::Command::new("/home/greg/.dotfiles/bin/theme").spawn()?;

        Ok(())
    }
}

fn main() -> Result<()> {
    flexi_logger::Logger::with_env().start()?;

    let cached_location = LocationInfo::get_cached();
    if let Some(cached_location) = cached_location {
        cached_location.get_theme().set()?;
    }

    let location = LocationInfo::estimate()?;
    log::info!("{:?}", location);

    // Immediately set the appropriate theme
    if !matches!(cached_location, Some(l) if l == location) {
        location.get_theme().set()?;
    }

    let mut prev_theme = location.get_theme();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));

        let theme = location.get_theme();
        if theme != prev_theme {
            theme.set()?;
            prev_theme = theme;
        }
    }
}
