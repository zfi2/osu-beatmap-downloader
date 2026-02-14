use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use rosu_v2::prelude::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::types::BeatmapInfo;

pub async fn fetch_most_played() -> Result<Vec<BeatmapInfo>> {
    let get_env = |key: &str, msg: &str| -> Result<String> {
        std::env::var(key).context(format!("{} - {}", key, msg))
    };

    let client_id = get_env("OSU_CLIENT_ID", "get it from https://osu.ppy.sh/home/account/edit#oauth")?;
    let client_secret = get_env("OSU_CLIENT_SECRET", "not set")?;
    let user_id = get_env("OSU_USERNAME", "put your osu username here")?;

    println!("Authenticating with osu! API...");
    
    let osu = Osu::builder()
        .client_id(client_id.parse()?)
        .client_secret(client_secret)
        .build()
        .await?;

    println!("Authenticated successfully! Fetching maps...");

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );

    let mut all_maps = Vec::new();
    let mut offset = 0;
    const LIMIT: usize = 50; // 50 is the limit for the 'most_played' field

    loop {
        pb.set_message(format!("Fetched {} maps...", all_maps.len()));
        
        let maps: Vec<MostPlayedMap> = osu
            .user_most_played(&user_id)
            .limit(LIMIT)
            .offset(offset)
            .await?;

        let batch_size = maps.len();
        if batch_size == 0 {
            break;
        }

        for map in maps {
            let beatmap_info = BeatmapInfo {
                beatmap_id: map.map_id,
                beatmapset_id: map.mapset.mapset_id,
                title: map.mapset.title.to_string(),
                artist: map.mapset.artist.to_string(),
                version: map.map.version.to_string(),
                play_count: map.count as u32,
                download_link: format!("https://osu.ppy.sh/beatmapsets/{}", map.mapset.mapset_id),
            };
            all_maps.push(beatmap_info);
        }

        if batch_size < LIMIT {
            break;
        }
        
        offset += batch_size;
        pb.tick();
        // be polite to the API :3
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    pb.finish_with_message(format!("Fetched {} maps total!", all_maps.len()));

    Ok(all_maps)
}

pub fn save_beatmaps(maps: &[BeatmapInfo], path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(maps)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

pub fn load_beatmaps(path: &Path) -> Result<Vec<BeatmapInfo>> {
    let file_content = std::fs::read_to_string(path)
        .context("Failed to read JSON file")?;
    let maps: Vec<BeatmapInfo> = serde_json::from_str(&file_content)?;
    Ok(maps)
}