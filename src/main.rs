use anyhow::Result;
use chrono::{DateTime, Local, NaiveTime, Utc};

#[derive(Debug)]
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
                &format!("IS_DAY={}", matches!(self, Theme::Day)),
            ])
            .spawn()?;

        let replacements = maplit::hashmap! {
            "/home/greg/.config/gtk-3.0/settings.ini" => ("Flat-Remix-GTK-Blue-Dark-Solid", "Flat-Remix-GTK-Blue-Solid"),
            "/home/greg/.config/xsettingsd/xsettingsd.conf" => ("Flat-Remix-GTK-Blue-Dark-Solid", "Flat-Remix-GTK-Blue-Solid"),
            "/home/greg/.config/gtk-3.0/settings.ini" => ("Papirus-Dark", "Papirus-Light"),
            "/home/greg/.dotfiles/kitty/active_theme.conf" => ("dark.conf", "light.conf"),
        };

        replacements
            .iter()
            .map(|(path, r)| match self {
                Theme::Day => (path, r.0, r.1),
                Theme::Night => (path, r.1, r.0),
            })
            .for_each(|(path, find, replace)| {
                std::process::Command::new("sd")
                    .args(&[find, replace, path])
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| eprintln!("{}: {}", path, e))
                    .unwrap_or_default()
            });

        std::fs::read_dir("/tmp/")
            .map(|sockets| {
                #[allow(unused_must_use)]
                sockets
                    .filter_map(|p| p.ok())
                    .filter(|d| d.file_name().to_str().unwrap().starts_with("kitty-socket-"))
                    .for_each(|socket| {
                        std::process::Command::new("kitty")
                            .args(&[
                                "@",
                                &format!("--to=unix:{}", socket.path().to_str().unwrap()),
                                "set-colors",
                                "-a",
                                match self {
                                    Theme::Night => "~/.dotfiles/kitty/dark.conf",
                                    Theme::Day => "~/.dotfiles/kitty/light.conf",
                                },
                            ])
                            .spawn()
                            .map_err(|e| log::debug!("Error on socket {:?}: {}", socket, e));
                    })
            })
            .unwrap_or(());

        std::process::Command::new("nvim-ctrl")
            .args(&[&format!("let $IS_DAY=\"{}\"", matches!(self, Theme::Day))])
            .spawn()?;

        std::process::Command::new("nvim-ctrl")
            .args(&["source ~/.dotfiles/nvim/init.vim"])
            .spawn()?;

        std::process::Command::new("systemctl")
            .args(&["--user", "restart", "wallpaper"])
            .spawn()?;

        std::process::Command::new("fish")
            .arg("/home/greg/.dotfiles/bin/theme")
            .spawn()?;

        std::process::Command::new("systemctl")
            .args(&["--user", "reload-or-restart", "xsettingsd"])
            .spawn()?;

        Ok(())
    }
}

fn main() -> Result<()> {
    flexi_logger::Logger::with_env().start()?;

    let location = LocationInfo::estimate()?;
    log::info!("{:?}", location);

    // Immediately set the appropriate theme
    let mut prev_theme = location.get_theme();
    prev_theme.set()?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));

        let theme = location.get_theme();
        if theme != prev_theme {
            theme.set()?;
        }

        prev_theme = theme;
    }
}
