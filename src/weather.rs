use crate::{
    app::App,
    paint::{Card, H},
    theme::{self, W},
};
use eframe::egui::{self, Color32};
use serde::Deserialize;
use std::{
    sync::mpsc::{self, TryRecvError},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
pub(crate) struct City {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    admin1: Option<String>,
    #[serde(default)]
    admin2: Option<String>,
    #[serde(default)]
    country: Option<String>,
}
impl City {
    fn label(&self) -> String {
        let mut parts = vec![self.name.as_str()];
        for part in [&self.admin2, &self.admin1, &self.country]
            .into_iter()
            .flatten()
        {
            if !part.is_empty()
                && !parts
                    .iter()
                    .any(|existing| part.contains(existing) || existing.contains(part.as_str()))
            {
                parts.push(part);
            }
        }
        parts.join(" · ")
    }
}
#[derive(Deserialize)]
struct Cities {
    #[serde(default)]
    results: Vec<City>,
}
#[derive(Deserialize)]
pub(crate) struct Forecast {
    #[serde(rename = "temperature_2m")]
    pub temperature: f64,
    #[serde(rename = "relative_humidity_2m")]
    humidity: Option<u32>,
    #[serde(rename = "weather_code")]
    code: u32,
    #[serde(rename = "wind_speed_10m")]
    wind: Option<f64>,
    #[serde(rename = "apparent_temperature")]
    pub apparent: Option<f64>,
    #[serde(default)]
    is_day: Option<u8>,
    #[serde(skip)]
    pub daily: Vec<DailyForecast>,
}
#[derive(Default, Deserialize)]
struct DailyResponse {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    weather_code: Vec<Option<u32>>,
    #[serde(default)]
    temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<u32>>,
}
#[derive(Debug)]
pub(crate) struct DailyForecast {
    date: String,
    code: Option<u32>,
    pub high: Option<f64>,
    low: Option<f64>,
    pub rain_probability: Option<u32>,
}
#[derive(Deserialize)]
struct ForecastResponse {
    current: Forecast,
    #[serde(default)]
    daily: DailyResponse,
}
pub(crate) fn parse_city(body: &str) -> Result<City, String> {
    serde_json::from_str::<Cities>(body)
        .map_err(|_| "城市数据格式错误".to_owned())?
        .results
        .into_iter()
        .next()
        .ok_or_else(|| "未找到该城市".to_owned())
}
pub(crate) fn parse_forecast(body: &str) -> Result<Forecast, String> {
    let mut response = serde_json::from_str::<ForecastResponse>(body)
        .map_err(|_| "天气数据格式错误".to_owned())?;
    let daily = &response.daily;
    response.current.daily = daily
        .time
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, date)| DailyForecast {
            date: date.clone(),
            code: daily.weather_code.get(i).copied().flatten(),
            high: daily.temperature_2m_max.get(i).copied().flatten(),
            low: daily.temperature_2m_min.get(i).copied().flatten(),
            rain_probability: daily
                .precipitation_probability_max
                .get(i)
                .copied()
                .flatten(),
        })
        .collect();
    Ok(response.current)
}
impl Forecast {
    pub(crate) fn description(&self) -> &'static str {
        description(self.code)
    }
    fn is_day(&self) -> bool {
        self.is_day != Some(0)
    }
}
fn description(code: u32) -> &'static str {
    match code {
        0 => "晴",
        1..=3 => "多云",
        45 | 48 => "雾",
        51 | 53 | 55 => "毛毛雨",
        56 | 57 => "冻毛毛雨",
        61 => "小雨",
        63 => "中雨",
        65 => "大雨",
        66 | 67 => "冻雨",
        71 | 73 | 75 | 77 => "雪",
        80..=82 => "阵雨",
        85 | 86 => "阵雪",
        95..=99 => "雷雨",
        _ => "未知天气",
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Sun,
    Moon,
    PartlyCloudy,
    Cloud,
    Fog,
    Rain,
    Snow,
    Thunder,
    Unknown,
}
pub(crate) fn icon_for(code: u32, day: bool) -> Icon {
    match code {
        0 if day => Icon::Sun,
        0 => Icon::Moon,
        1 | 2 => Icon::PartlyCloudy,
        3 => Icon::Cloud,
        45 | 48 => Icon::Fog,
        51..=57 | 61..=67 | 80..=82 => Icon::Rain,
        71 | 73 | 75 | 77 | 85 | 86 => Icon::Snow,
        95..=99 => Icon::Thunder,
        _ => Icon::Unknown,
    }
}
pub(crate) fn format_temperature(celsius: f64, fahrenheit: bool) -> String {
    let value = if fahrenheit {
        celsius * 1.8 + 32.0
    } else {
        celsius
    };
    // 让接近零的温度显示为 0°，避免出现 -0°。
    let rounded = value.round();
    format!("{:.0}°", if rounded == 0.0 { 0.0 } else { rounded })
}

/// 独立矢量图标，收起态、预览、侧边与天气页共用。
fn draw_icon(c: &Card, x: f32, y: f32, size: f32, kind: Icon, day: bool) {
    let card = c.scaled(x, y, size / 18.0);
    let c = &card;
    let yellow = Color32::from_rgb(255, 201, 92);
    let cloud = Color32::from_rgb(222, 232, 241);
    let blue = Color32::from_rgb(96, 184, 255);
    let sun = |c: &Card, sx: f32, sy: f32| {
        c.circle(sx, sy, 3.4, yellow);
        for (dx, dy) in [
            (0.0, -7.0),
            (0.0, 7.0),
            (-7.0, 0.0),
            (7.0, 0.0),
            (-5.0, -5.0),
            (5.0, -5.0),
            (-5.0, 5.0),
            (5.0, 5.0),
        ] {
            c.seg(
                (sx + dx * 0.72, sy + dy * 0.72),
                (sx + dx, sy + dy),
                1.2,
                yellow,
            );
        }
    };
    let moon = |c: &Card, sx: f32, sy: f32| {
        c.arc(
            sx,
            sy,
            5.0,
            -1.35,
            1.55,
            1.8,
            Color32::from_rgb(255, 221, 155),
        );
        c.arc(
            sx + 2.5,
            sy - 1.2,
            4.0,
            1.4,
            3.9,
            1.8,
            Color32::from_rgb(255, 221, 155),
        );
    };
    let cloud_shape = |c: &Card, cy: f32| {
        c.circle(x - 4.0, cy + 0.8, 3.2, cloud);
        c.circle(x, cy - 1.2, 4.6, cloud);
        c.circle(x + 4.0, cy + 0.8, 3.1, cloud);
        c.fill(x - 7.0, cy + 0.5, 14.0, 3.5, 1.7, cloud);
    };
    match kind {
        Icon::Sun => sun(c, x, y),
        Icon::Moon => moon(c, x, y),
        Icon::PartlyCloudy => {
            if day {
                sun(c, x - 3.0, y - 3.0);
            } else {
                moon(c, x - 3.0, y - 3.0);
            }
            cloud_shape(c, y + 2.0);
        }
        Icon::Cloud => cloud_shape(c, y),
        Icon::Fog => {
            cloud_shape(c, y - 3.0);
            c.seg((x - 6.0, y + 4.0), (x + 6.0, y + 4.0), 1.2, cloud);
            c.seg((x - 4.0, y + 7.0), (x + 4.0, y + 7.0), 1.2, cloud);
        }
        Icon::Rain | Icon::Snow | Icon::Thunder => {
            cloud_shape(c, y - 3.0);
            match kind {
                Icon::Rain => {
                    for dx in [-4.0, 0.0, 4.0] {
                        c.seg((x + dx + 1.0, y + 3.0), (x + dx, y + 6.0), 1.5, blue);
                    }
                }
                Icon::Snow => {
                    for dx in [-4.0, 0.0, 4.0] {
                        c.circle(x + dx, y + 5.0, 1.0, blue);
                    }
                }
                Icon::Thunder => c.poly(
                    &[
                        (x + 1.0, y + 1.0),
                        (x - 2.0, y + 5.0),
                        (x + 0.2, y + 5.0),
                        (x - 1.0, y + 9.0),
                        (x + 4.0, y + 3.5),
                        (x + 1.5, y + 3.5),
                    ],
                    yellow,
                ),
                _ => {}
            }
        }
        Icon::Unknown => {
            c.ring(x, y, 6.5, 1.1, Color32::from_gray(160));
            c.centered_text(x, y, "?", 9.0, W::Bold, Color32::from_gray(190), H::Center);
        }
    }
}

fn fetch(city: &str) -> Result<(String, Forecast), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=zh&format=json",
        url::form_urlencoded::byte_serialize(city.as_bytes()).collect::<String>()
    );
    let body = agent
        .get(&url)
        .call()
        .map_err(|_| "城市查询失败，请检查网络".to_owned())?
        .into_string()
        .map_err(|_| "城市数据读取失败".to_owned())?;
    let location = parse_city(&body)?;
    let url = format!("https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,is_day&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days=3", location.latitude, location.longitude);
    let body = agent
        .get(&url)
        .call()
        .map_err(|_| "天气获取失败，请检查网络".to_owned())?
        .into_string()
        .map_err(|_| "天气数据读取失败".to_owned())?;
    Ok((location.label(), parse_forecast(&body)?))
}
type WeatherResult = Result<(String, Forecast), String>;
#[derive(Default)]
pub(crate) struct State {
    city: String,
    rx: Option<mpsc::Receiver<(String, WeatherResult)>>,
    result: Option<(String, Forecast)>,
    error: Option<String>,
    last_attempt: Option<Instant>,
    updated_at: Option<chrono::DateTime<chrono::Local>>,
    city_changed: Option<Instant>,
    refresh_requested: bool,
}
impl State {
    pub(crate) fn release(&mut self) {
        *self = Self::default();
    }
    fn display(&self, fahrenheit: bool) -> (Icon, bool, String) {
        match &self.result {
            Some((_, weather)) => (
                icon_for(weather.code, weather.is_day()),
                weather.is_day(),
                format_temperature(weather.temperature, fahrenheit),
            ),
            None => (
                Icon::Unknown,
                true,
                if self.error.is_some() {
                    "天气!"
                } else {
                    "天气"
                }
                .to_owned(),
            ),
        }
    }
    pub(crate) fn draw_resident(&self, c: &Card, x: f32, fahrenheit: bool) {
        let (icon, day, text) = self.display(fahrenheit);
        draw_icon(c, x - 20.0, 15.0, 21.0, icon, day);
        c.centered_text(
            x + 10.0,
            15.0,
            &text,
            14.0,
            W::Semibold,
            theme::WHITE,
            H::Center,
        );
    }
    pub(crate) fn draw_side(&self, c: &Card, x: f32, y: f32, fahrenheit: bool) {
        let (icon, day, text) = self.display(fahrenheit);
        draw_icon(c, x, y - 7.0, 14.0, icon, day);
        c.centered_text(
            x,
            y + 9.0,
            &text,
            10.0,
            W::Semibold,
            theme::WHITE,
            H::Center,
        );
    }
    fn accept(&mut self, requested: &str, result: WeatherResult) {
        if requested != self.city {
            return;
        }
        match result {
            Ok(data) => {
                self.result = Some(data);
                self.error = None;
                self.updated_at = Some(chrono::Local::now());
            }
            Err(message) => self.error = Some(message),
        }
    }
    pub(crate) fn refresh(&mut self) {
        if self.rx.is_none() {
            self.refresh_requested = true;
        }
    }
    pub(crate) fn update(&mut self, city: &str, interval: u32, ctx: &egui::Context) {
        let city = city.trim();
        if self.city != city {
            self.city = city.to_owned();
            self.result = None;
            self.error = None;
            self.updated_at = None;
            self.last_attempt = None;
            self.city_changed = Some(Instant::now());
            self.refresh_requested = false;
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok((requested, result)) => {
                    self.rx = None;
                    self.accept(&requested, result);
                }
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.error = Some("天气服务暂不可用".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let ready = self.refresh_requested
            || self.last_attempt.map_or_else(
                || {
                    self.city_changed
                        .map_or(true, |time| time.elapsed() >= Duration::from_millis(700))
                },
                |time| time.elapsed() >= Duration::from_secs(u64::from(interval) * 60),
            );
        if !city.is_empty() && self.rx.is_none() && ready {
            let Some(lease) = crate::resources::WEATHER.acquire() else {
                ctx.request_repaint_after(Duration::from_millis(500));
                return;
            };
            let requested = city.to_owned();
            let (tx, rx) = mpsc::channel();
            self.rx = Some(rx);
            self.last_attempt = Some(Instant::now());
            self.refresh_requested = false;
            let ctx = ctx.clone();
            lease.spawn(move || {
                let result = fetch(&requested);
                let _ = tx.send((requested, result));
                ctx.request_repaint();
            });
        }
        if self.rx.is_some() || (!city.is_empty() && self.last_attempt.is_none()) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
    #[cfg(test)]
    pub(crate) fn set_test_forecast(&mut self, city: &str, body: &str) {
        self.city = city.into();
        self.result = Some((city.into(), parse_forecast(body).unwrap()));
        self.last_attempt = Some(Instant::now());
        self.updated_at = Some(chrono::Local::now());
        self.error = None;
    }
}
impl App {
    pub(crate) fn weather_settings(&mut self, ui: &mut egui::Ui) {
        use crate::extras::{
            settings_card, settings_control_row_right, settings_hint, settings_label,
            settings_segmented,
        };
        settings_label(ui, "城市");
        settings_card(ui, |ui| {
            settings_control_row_right(ui, "天气城市", |ui| {
                ui.add_sized(
                    [230.0, 28.0],
                    egui::TextEdit::singleline(&mut self.settings.weather_city)
                        .hint_text("例如：芜湖"),
                );
            });
            settings_hint(
                ui,
                "输入城市名，自动匹配第一个结果；城市名称会发送至 Open-Meteo。匹配位置显示在下方。",
            );
            if let Some((city, _)) = &self.weather.result {
                settings_hint(ui, &format!("当前匹配：{city}"));
            }
            if let Some(error) = &self.weather.error {
                settings_hint(ui, error);
            }
        });
        settings_label(ui, "显示与更新");
        settings_card(ui, |ui| {
            settings_control_row_right(ui, "温度单位", |ui| {
                settings_segmented(
                    ui,
                    &mut self.settings.weather_fahrenheit,
                    &[(false, "摄氏 °C"), (true, "华氏 °F")],
                    76.0,
                );
            });
            ui.separator();
            settings_control_row_right(ui, "刷新间隔", |ui| {
                settings_segmented(
                    ui,
                    &mut self.settings.weather_interval,
                    &[(15, "15 分钟"), (30, "30 分钟"), (60, "60 分钟")],
                    70.0,
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.settings.weather_city.trim().is_empty() && self.weather.rx.is_none(),
                        egui::Button::new("立即刷新"),
                    )
                    .clicked()
                {
                    self.weather.refresh();
                }
                let status = if self.weather.rx.is_some() {
                    "正在更新…".to_owned()
                } else if let Some(time) = self.weather.updated_at {
                    format!("上次更新 {}", time.format("%m-%d %H:%M"))
                } else {
                    "尚未获取天气".to_owned()
                };
                ui.label(
                    egui::RichText::new(status)
                        .size(11.5)
                        .color(ui.visuals().weak_text_color()),
                );
            });
            settings_hint(ui, "天气的胶囊态和展开态可在「页面管理」中分别控制。");
        });
        settings_label(ui, "数据来源");
        settings_card(ui, |ui| {
            settings_hint(
                ui,
                "天气与城市匹配由 Open-Meteo 提供；天气预报可能变化，请以当地官方信息为准。",
            );
        });
    }

    pub(crate) fn page_weather(&mut self, c: &Card) {
        let fahrenheit = self.settings.weather_fahrenheit;
        c.text(15.0, 25.0, "天气", 14.0, W::Bold, theme::WHITE, H::Left);
        let refresh = c
            .hit(
                crate::app::CONTENT_W - 94.0,
                35.0,
                76.0,
                22.0,
                "weather-refresh",
            )
            .on_hover_text("立即刷新天气");
        c.text(
            crate::app::CONTENT_W - 22.0,
            51.0,
            if self.weather.rx.is_some() {
                "更新中…"
            } else {
                "刷新"
            },
            10.0,
            W::Regular,
            theme::BLUE,
            H::Right,
        );
        if refresh.clicked() {
            self.weather.refresh();
        }
        match &self.weather.result {
            Some((city, weather)) => {
                c.text(
                    20.0,
                    52.0,
                    &c.fit(city, 12.0, W::Semibold, 260.0),
                    12.0,
                    W::Semibold,
                    theme::WHITE,
                    H::Left,
                );
                c.text(
                    20.0,
                    105.0,
                    &format_temperature(weather.temperature, fahrenheit),
                    34.0,
                    W::Bold,
                    theme::WHITE,
                    H::Left,
                );
                draw_icon(
                    c,
                    155.0,
                    83.0,
                    30.0,
                    icon_for(weather.code, weather.is_day()),
                    weather.is_day(),
                );
                c.text(
                    183.0,
                    80.0,
                    weather.description(),
                    16.0,
                    W::Semibold,
                    theme::WHITE,
                    H::Left,
                );
                let feels = weather
                    .apparent
                    .map(|v| format_temperature(v, fahrenheit))
                    .unwrap_or_else(|| "--".into());
                c.text(
                    183.0,
                    102.0,
                    &format!(
                        "体感 {feels}  ·  湿度 {}",
                        weather
                            .humidity
                            .map(|v| format!("{v}%"))
                            .unwrap_or_else(|| "--".into())
                    ),
                    11.0,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
                c.text(
                    183.0,
                    120.0,
                    &format!(
                        "风速 {}",
                        weather
                            .wind
                            .map(|v| format!("{v:.0} km/h"))
                            .unwrap_or_else(|| "--".into())
                    ),
                    11.0,
                    W::Regular,
                    theme::GRAY,
                    H::Left,
                );
                for (index, day) in weather.daily.iter().enumerate() {
                    let x = 20.0 + index as f32 * 140.0;
                    c.fill(x, 135.0, 130.0, 65.0, 8.0, theme::CARD);
                    let label = match index {
                        0 => "今天",
                        1 => "明天",
                        _ => "后天",
                    };
                    let date = day.date.get(5..).unwrap_or(&day.date);
                    c.text(
                        x + 9.0,
                        151.0,
                        &format!("{label} {date}"),
                        10.0,
                        W::Regular,
                        theme::GRAY,
                        H::Left,
                    );
                    draw_icon(
                        c,
                        x + 18.0,
                        177.0,
                        16.0,
                        day.code
                            .map(|code| icon_for(code, true))
                            .unwrap_or(Icon::Unknown),
                        true,
                    );
                    let high = day
                        .high
                        .map(|v| format_temperature(v, fahrenheit))
                        .unwrap_or_else(|| "--".into());
                    let low = day
                        .low
                        .map(|v| format_temperature(v, fahrenheit))
                        .unwrap_or_else(|| "--".into());
                    c.text(
                        x + 34.0,
                        180.0,
                        &format!("{high} / {low}"),
                        11.0,
                        W::Semibold,
                        theme::WHITE,
                        H::Left,
                    );
                    if let Some(probability) = day.rain_probability {
                        c.text(
                            x + 121.0,
                            194.0,
                            &format!("最高降水概率 {probability}%"),
                            8.0,
                            W::Regular,
                            theme::GRAY,
                            H::Right,
                        );
                    }
                }
                let updated = self
                    .weather
                    .updated_at
                    .map(|t| format!("更新 {}", t.format("%H:%M")))
                    .unwrap_or_default();
                let status = if self.weather.error.is_some() {
                    "更新失败，显示上次数据".to_owned()
                } else {
                    format!("{updated} · Open-Meteo")
                };
                c.text(20.0, 216.0, &status, 9.0, W::Regular, theme::GRAY, H::Left);
            }
            None => {
                let status = if self.settings.weather_city.trim().is_empty() {
                    "请在天气设置中输入城市"
                } else if let Some(error) = &self.weather.error {
                    error.as_str()
                } else {
                    "正在获取天气…"
                };
                c.text(28.0, 100.0, status, 13.0, W::Regular, theme::GRAY, H::Left);
                if self.weather.error.is_some() {
                    c.text(
                        28.0,
                        126.0,
                        "可检查城市名称或点击刷新",
                        11.0,
                        W::Regular,
                        theme::GRAY,
                        H::Left,
                    );
                }
                if self.settings.weather_city.trim().is_empty() {
                    c.text(
                        28.0,
                        129.0,
                        "打开天气设置",
                        11.0,
                        W::Regular,
                        theme::BLUE,
                        H::Left,
                    );
                    if c.hit(20.0, 110.0, 120.0, 30.0, "weather-open-settings")
                        .clicked()
                    {
                        self.settings_tab = 12;
                        self.open_settings();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_failure_preserves_last_good_data_and_ignores_old_city() {
        let mut state = State::default();
        state.set_test_forecast(
            "芜湖",
            r#"{"current":{"temperature_2m":18.0,"weather_code":0}}"#,
        );
        let updated = state.updated_at;
        state.accept("芜湖", Err("网络异常".into()));
        assert!(state.result.is_some());
        assert_eq!(state.error.as_deref(), Some("网络异常"));
        assert_eq!(state.updated_at, updated);
        state.accept("其他城市", Err("过期请求".into()));
        assert_eq!(state.error.as_deref(), Some("网络异常"));
    }

    #[test]
    fn incomplete_daily_arrays_do_not_panic_or_misalign() {
        let weather=parse_forecast(r#"{"current":{"temperature_2m":12.0,"weather_code":3},"daily":{"time":["2026-11-16","2026-11-17"],"temperature_2m_max":[null,20.0],"weather_code":[3]}}"#).unwrap();
        assert_eq!(weather.daily.len(), 2);
        assert_eq!(weather.daily[0].high, None);
        assert_eq!(weather.daily[1].high, Some(20.0));
        assert_eq!(weather.daily[1].code, None);
        assert_eq!(weather.daily[1].rain_probability, None);
    }
}
