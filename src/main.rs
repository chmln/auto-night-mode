use anyhow::{anyhow as err, Result};
use chrono::{DateTime, Local, NaiveTime};
use tokio::time::{interval, Duration};

pub struct LocationInfo {
    sunset: NaiveTime,
    sunrise: NaiveTime,
}

impl LocationInfo {
    pub fn get_theme(&self) -> Theme {
        let now = Local::now().time();

        if now > self.sunset || now < self.sunrise {
            Theme::Night
        } else {
            Theme::Day
        }
    }

    async fn estimate() -> Result<Self> {
        let lat_lng = surf::get("https://ipinfo.io")
            .recv_json::<serde_json::Value>()
            .await
            .unwrap()
            .get_mut("loc").ok_or(err!("Invalid response"))?
            .as_str()
            .ok_or(err!("Error retrieving lat lng"))?.to_owned();


        let loc: Vec<f64> = lat_lng
            .split(",")
            .map(|x| x.parse::<f64>().unwrap())
            .collect();

        #[derive(serde::Deserialize)]
        struct Times {
            sunset: DateTime<Local>,
            sunrise: DateTime<Local>,
        }

        let Times { sunset, sunrise } = serde_json::from_value(
            surf::get(format!(
                "https://api.sunrise-sunset.org/json?lat={}&lng={}&formatted=0",
                loc[0], loc[1]
            ))
            .recv_json::<serde_json::Value>()
            .await.map_err(|_| err!("Invalid json"))?
            .get_mut("results").ok_or(err!("Invalid json"))?
            .take(),
        )?;

        Ok(Self {
            sunset: sunset.time(),
            sunrise: sunrise.time(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Theme {
    Night,
    Day
}

impl Theme {
    fn set(&self) -> Result<()> {
        let gtk_config_path = format!("{}/.config/gtk-3.0/settings.ini", std::env::var("HOME")?);
        let find = if let Theme::Night = self { false } else { true };
        let replace = !find;

        println!("Applying theme: {:?}", self);

        std::fs::write(
            &gtk_config_path,
            std::fs::read_to_string(&gtk_config_path)?.replace(
                &format!("gtk-application-prefer-dark-theme={}", find),
                &format!("gtk-application-prefer-dark-theme={}", replace),
            ),
        )?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let location = LocationInfo::estimate().await?;
    let mut timer = interval(Duration::from_secs(60));

    // Immediately set the appropriate theme
    let mut prev_theme = location.get_theme();
    prev_theme.set()?;

    loop {
        timer.tick().await;

        let theme = location.get_theme();
        if theme != prev_theme {
            theme.set()?;
        }

        prev_theme = theme;
    }
}
