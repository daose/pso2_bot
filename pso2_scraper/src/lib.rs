use chrono::offset::LocalResult::Single;
use chrono::offset::TimeZone;
use chrono::offset::Utc;
use chrono::DateTime;
use chrono::Duration;
use chrono_tz::US::Pacific;
use log::{error, info};
use regex::Regex;
use scraper::element_ref::ElementRef;
use scraper::selector::Selector;
use std::collections::HashMap;
use std::str::FromStr;
#[macro_use]
extern crate lazy_static;

const QUESTS_ENDPOINT: &str = "https://pso2.com/news/urgent-quests";

lazy_static! {
    static ref NEWS_REGEX: Regex = Regex::new(r"ShowDetails\('(.+?)'").unwrap();
    static ref COLOR_REGEX: Regex = Regex::new(r"background:\s*(.+?);").unwrap();
    static ref RGB_REGEX: Regex = Regex::new(r"rgb\((\d+),\s*(\d+),\s*(\d+)\)").unwrap();
    static ref QUESTS_SELECTOR: Selector =
        Selector::parse(".emergency-section .news-item .image").unwrap();
    static ref TBODY: Selector = Selector::parse("tbody").unwrap();
    static ref TR: Selector = Selector::parse("tr").unwrap();
    static ref TD: Selector = Selector::parse("td").unwrap();
    static ref OG_URL: Selector = Selector::parse("meta[property=\"og:url\"]").unwrap();
}

#[derive(Debug)]
pub struct Quest {
    pub start_time: DateTime<Utc>,
    pub name: String,
}

fn rgb_to_hex(color: &str) -> Option<String> {
    if let Some(rgb) = RGB_REGEX.captures(color) {
        if rgb.len() == 4 {
            let [r, g, b] = [
                rgb.get(1).unwrap(),
                rgb.get(2).unwrap(),
                rgb.get(3).unwrap(),
            ];
            return Some(format!(
                "#{:x}{:x}{:x}",
                u8::from_str(r.as_str()).unwrap(),
                u8::from_str(g.as_str()).unwrap(),
                u8::from_str(b.as_str()).unwrap()
            ));
        }
    }
    None
}

/// Assumes PDT
fn parse_calendar(tbody: &ElementRef, map: HashMap<String, String>, quests: &mut Vec<Quest>) {
    // could have colspan = 2
    // find out width of table first
    // [date, date, date, date, date,...]assign indexes to
    // int column, default 1 or increment by colspan
    // read first column for the time of day
    let mut col_date = Vec::new();
    let mut trs = tbody.select(&TR);
    if let Some(tds) = trs.next().map(|tr| tr.select(&TD).skip(1))
    // skip "week" data
    {
        for td in tds {
            // TODO: when the year switches, they will post update in december for january of the
            // next year but we will report the wrong year (current year) instead of the next year.
            // also if we read the same month for two different years it'll mess things up too
            let date = td.text().collect::<String>();
            if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(
                format!("{}/{}", chrono::Utc::now().date().format("%Y"), date).as_str(),
                "%Y/%m/%d",
            ) {
                let colspan = td
                    .value()
                    .attr("colspan")
                    .map_or(1, |colspan| usize::from_str(colspan).unwrap_or(1));
                for _ in 0..colspan {
                    col_date.push(naive_date);
                }
            }
        }
        for tr in trs {
            let mut tds = tr.select(&TD);
            if let Some(td) = tds.next() {
                if let Ok(time) = chrono::NaiveTime::parse_from_str(
                    td.text().collect::<String>().as_str(),
                    "%I:%M %p",
                ) {
                    for (i, td) in tds.enumerate() {
                        if let Some(style) = td.value().attr("style") {
                            if let Some(captures) = COLOR_REGEX.captures(style) {
                                if let Some(color) = captures.get(1) {
                                    if let Some(event) =
                                        map.get(&color.as_str().trim().to_lowercase())
                                    {
                                        if let Some(date) = col_date.get(i) {
                                            if let Single(datetime) =
                                                Pacific.from_local_datetime(&date.and_time(time))
                                            {
                                                quests.push(Quest {
                                                    start_time: datetime.with_timezone(&Utc),
                                                    name: event.to_string(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn parse_news_site(html: &str, quests: &mut Vec<Quest>) {
    let document = scraper::Html::parse_document(&html);
    let url: &str = document
        .select(&OG_URL)
        .next()
        .and_then(|meta| meta.value().attr("content"))
        .unwrap_or("");

    // filter tbody into [(legend,calendar)]
    let calendars = document
        .select(&TBODY)
        .filter(|table| {
            table
                .first_child()
                .map_or(false, |tr| tr.children().count() == 2)
        })
        .zip(document.select(&TBODY).filter(|table| {
            table
                .first_child()
                .map_or(false, |tr| tr.children().count() == 8)
        }));

    for (legend, calendar) in calendars {
        let mut map = HashMap::new();
        for item in legend.select(&TR) {
            let mut tds = item.select(&TD);
            if let Some(style) = tds.next().unwrap().value().attr("style") {
                if let Some(captures) = COLOR_REGEX.captures(style) {
                    if let Some(m) = captures.get(1) {
                        let color = {
                            let css_color = m.as_str();
                            if css_color.starts_with('r') {
                                if let Some(hex) = rgb_to_hex(css_color) {
                                    hex
                                } else {
                                    String::from_str(css_color).unwrap()
                                }
                            } else {
                                String::from_str(css_color).unwrap()
                            }
                        };
                        let event: String = tds.next().unwrap().text().into_iter().collect();
                        map.insert(color.trim().to_lowercase(), format!("{}: {}", event, url));
                    }
                }
            } else {
                error!("Unable to find style attribute");
            }
        }
        parse_calendar(&calendar, map, quests);
    }
}

/*
 * https://pso2.com/news/urgent-quests
 * .emergency-section .news-item .image
 */

// store new -> old order
pub fn fetch_urgent_quests() -> Vec<Quest> {
    let mut quests = Vec::<Quest>::new();
    let client = reqwest::blocking::Client::new();
    let result = client.get(QUESTS_ENDPOINT).send();
    if let Ok(response) = result {
        if let Ok(html) = response.text() {
            let document = scraper::Html::parse_document(&html);
            let quests_elements = document.select(&QUESTS_SELECTOR);
            for quest in quests_elements {
                if let Some(attr) = quest.value().attr("onclick") {
                    if let Some(capture) = NEWS_REGEX.captures(attr) {
                        if let Some(path) = capture.get(1) {
                            let url = format!("{}/{}", QUESTS_ENDPOINT, path.as_str());
                            let result = client.get(&url).send();
                            if let Ok(response) = result {
                                if let Ok(html) = response.text() {
                                    info!("Parsing {}", url);
                                    parse_news_site(&html, &mut quests);
                                }
                            }
                        }
                    }
                }
            }
        } else {
            error!("Error parsing response from: {}", QUESTS_ENDPOINT);
        }
    } else {
        error!("Error making request to: {}", QUESTS_ENDPOINT);
    }
    quests.sort_unstable_by_key(|quest| quest.start_time);
    quests.reverse();
    quests
}
