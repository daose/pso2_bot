use log::{error, info, warn};
use serenity::client::Client;
use serenity::prelude::EventHandler;

struct Handler;
impl EventHandler for Handler {}

// invite link: https://discord.com/api/oauth2/authorize?client_id=718292535591567360&scope=bot&permissions=3072
// client_id is the app's id
// permissions check the bot page for what permissions we need
use chrono::Utc;
use serenity::http::GuildPagination;
use serenity::model::id::GuildId;
use std::env;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// Interval between scraping pso2 site
const SCRAPE_INTERVAL: Duration = Duration::from_secs(60 * 24 * 24);
/// Interval between waking up to check if any quests are ready
const CHECK_INTERVAL: Duration = Duration::from_secs(60);
/// How much time in advance to send notification
const REMINDER_MINS: i64 = 15;
const BOT_CHANNEL: &str = "pso2_bot";

fn say_discord(quest: &pso2_scraper::Quest) {
    let client = Client::new(
        &env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN not found"),
        Handler,
    )
    .expect("Error creating client");

    let http = client.cache_and_http.http.clone();
    // TODO: support more than 100 guilds
    let guilds = http
        .get_guilds(&GuildPagination::After(GuildId(1)), 100)
        .expect("Unable to fetch guilds");
    if guilds.len() >= 90 {
        warn!(
            "Number of guilds: {}, require pagination support to support up to 100 guilds",
            guilds.len()
        );
    }

    for guild in guilds {
        info!("Processing guild: {}", guild.id);
        if let Ok(channels) = guild.id.channels(http.clone()) {
            if let Some(channel) = channels
                .values()
                .find(|channel| channel.name == BOT_CHANNEL)
            {
                channel
                    .say(
                        http.clone(),
                        format!(
                            "IN {} MINUTES: {}",
                            (quest.start_time - Utc::now()).num_minutes(),
                            quest.name
                        ),
                    )
                    .expect("Unable to send message");
            }
            info!("Completed sending message to guild: {}", guild.id);
        } else {
            error!("Unable to fetch channel list for guild: {}", guild.id);
        }
    }
}

fn main() {
    env_logger::init();

    let quests = Arc::new(Mutex::new(Vec::new()));
    let scraper_handle = {
        let quests = Arc::clone(&quests);
        thread::spawn(move || loop {
            info!("Updating PSO quests");
            let mut new_quests = pso2_scraper::fetch_urgent_quests();
            {
                let mut quests_guard = quests.lock().expect("Quests lock is poisoned");
                quests_guard.clear();
                quests_guard.append(&mut new_quests);
            }
            thread::sleep(SCRAPE_INTERVAL);
        })
    };

    let quests = Arc::clone(&quests);
    let discord_handle = thread::spawn(move || loop {
        info!("Discord thread wake");
        let mut quests_guard = quests.lock().expect("Quest lock is poisoned");
        while let Some(quest) = quests_guard.last() {
            let time_to_event = quest.start_time - Utc::now();
            info!("Processing quest: {:?}", quest);
            if time_to_event < chrono::Duration::seconds(0) {
                // event already happened
                quests_guard.pop().unwrap();
                info!("ignored");
            } else if time_to_event < chrono::Duration::minutes(REMINDER_MINS) {
                let quest = quests_guard.pop().unwrap();
                say_discord(&quest);
                info!("said");
            } else {
                info!("deal with later");
                break;
            }
        }

        info!("Discord thread sleep");
        thread::sleep(CHECK_INTERVAL);
    });

    scraper_handle.join().unwrap();
    discord_handle.join().unwrap();
}
