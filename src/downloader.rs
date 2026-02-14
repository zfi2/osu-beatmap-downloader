use anyhow::Result;
use futures_util::{StreamExt, stream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::{header::HeaderMap, Client};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::types::BeatmapInfo;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mirror {
    Nerinyan,
    Catboy,
}

impl Mirror {
    fn from_env() -> Self {
        let use_alt = std::env::var("USE_ALTERNATIVE_MIRROR")
            .unwrap_or_default()
            .to_lowercase();
        
        if use_alt == "true" || use_alt == "yes" || use_alt == "1" {
            Mirror::Catboy
        } else {
            Mirror::Nerinyan
        }
    }

    fn download_url(&self, beatmapset_id: u32) -> String {
        match self {
            Mirror::Nerinyan => format!("https://api.nerinyan.moe/d/{}", beatmapset_id),
            Mirror::Catboy => format!("https://catboy.best/d/{}", beatmapset_id),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Mirror::Nerinyan => "Nerinyan",
            Mirror::Catboy => "Catboy",
        }
    }
}

// catboy.best API structs
#[derive(Debug, Deserialize)]
struct CatboyRateLimitResponse {
    remaining: CatboyRemaining,
    types: CatboyTypes,
}

#[derive(Debug, Deserialize)]
struct CatboyRemaining { download: u32 }
#[derive(Debug, Deserialize)]
struct CatboyTypes { download: u32 }

/// rate limiter
struct RateLimiter {
    mirror: Mirror,
    // group mutable state into one mutex to reduce lock contention and complexity
    state: Mutex<RateLimitState>,
    client: Client,
}

struct RateLimitState {
    remaining: u32,
    reset_at: Instant,
    limit_cap: u32,
    download_count: u32,
}

impl RateLimiter {
    fn new(mirror: Mirror, client: Client) -> Self {
        Self {
            mirror,
            state: Mutex::new(RateLimitState {
                remaining: 60,
                reset_at: Instant::now() + Duration::from_secs(60),
                limit_cap: 60,
                download_count: 0,
            }),
            client,
        }
    }

    /// wait until we're allowed to make a request
    async fn wait(&self) {
        loop {
            let mut state = self.state.lock().await;
            
            // if tokens remaining, consume one and proceed
            if state.remaining > 0 {
                state.remaining -= 1;
                return;
            }

            // if time window passed, reset
            if Instant::now() >= state.reset_at {
                state.remaining = state.limit_cap;
                state.reset_at = Instant::now() + Duration::from_secs(60);
                continue;
            }

            // otherwise wait until reset
            let sleep_time = state.reset_at.duration_since(Instant::now()) + Duration::from_millis(100);
            drop(state); // drop lock before sleeping
            tokio::time::sleep(sleep_time).await;
        }
    }

    /// update limits based on response headers (nerinyan.moe)
    async fn update_from_headers(&self, headers: &HeaderMap) {
        if self.mirror == Mirror::Nerinyan {
            let mut state = self.state.lock().await;
            
            if let Some(rem) = get_header_u32(headers, "x-ratelimit-remaining-minute") {
                state.remaining = rem;
            }
            if let Some(cap) = get_header_u32(headers, "x-ratelimit-limit-minute") {
                state.limit_cap = cap;
            }
            if let Some(secs) = get_header_u64(headers, "x-ratelimit-reset")
                .or_else(|| get_header_u64(headers, "retry-after")) 
            {
                state.reset_at = Instant::now() + Duration::from_secs(secs + 1);
            }
        }
    }

    /// explicitly fetch limits (catboy.best)
    async fn refresh_catboy_limits(&self) -> Result<()> {
        if self.mirror == Mirror::Catboy {
            let response = self.client.get("https://catboy.best/api/ratelimits").send().await?;
            if response.status().is_success() {
                let data: CatboyRateLimitResponse = response.json().await?;
                let mut state = self.state.lock().await;
                state.remaining = data.remaining.download;
                state.limit_cap = data.types.download;
                state.reset_at = Instant::now() + Duration::from_secs(60);
            }
        }
        Ok(())
    }

    async fn on_download_complete(&self) {
        if self.mirror == Mirror::Catboy {
            let mut needs_refresh = false;
            {
                let mut state = self.state.lock().await;
                state.download_count += 1;
                if state.download_count % 50 == 0 {
                    needs_refresh = true;
                }
            }
            if needs_refresh {
                let _ = self.refresh_catboy_limits().await;
            }
        }
    }
}

// helper to parse headers
fn get_header_u32(h: &HeaderMap, key: &str) -> Option<u32> {
    h.get(key)?.to_str().ok()?.parse().ok()
}
fn get_header_u64(h: &HeaderMap, key: &str) -> Option<u64> {
    h.get(key)?.to_str().ok()?.parse().ok()
}

async fn download_beatmap(
    client: &Client,
    beatmap: &BeatmapInfo,
    output_dir: &Path,
    mirror: Mirror,
    rate_limiter: &RateLimiter,
    pb: &ProgressBar,
) -> Result<()> {
    let filename = beatmap.filename();
    let filepath = output_dir.join(&filename);

    let url = mirror.download_url(beatmap.beatmapset_id);
    let mut retry_count = 0;
    const MAX_RETRIES: u32 = 5;

    loop {
        rate_limiter.wait().await;

        let msg = if retry_count > 0 {
            format!("Retry {}/{} for {}", retry_count, MAX_RETRIES, beatmap.title) 
        } else {
            format!("Downloading {}", beatmap.title)
        };
        pb.set_message(msg);

        let response = client.get(&url).send().await?;
        rate_limiter.update_from_headers(response.headers()).await;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            if retry_count >= MAX_RETRIES {
                return Err(anyhow::anyhow!("Hit rate limit too many times"));
            }
            let wait_secs = get_header_u64(response.headers(), "retry-after").unwrap_or(10);
            pb.set_message(format!("Rate limited. Waiting {}s...", wait_secs));
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
            retry_count += 1;
            continue;
        }

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed: HTTP {}", response.status()));
        }

        let mut file = File::create(&filepath)?;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?)?;
        }

        rate_limiter.on_download_complete().await;
        pb.set_message(format!("Downloaded {}", beatmap.title));
        return Ok(());
    }
}

pub async fn download_beatmaps(maps: &[BeatmapInfo], output_dir: &Path) -> Result<()> {
    let mirror = Mirror::from_env();
    println!("osu! beatmap downloader ({} mirror)", mirror.name());
    println!("==========================================\n");

    fs::create_dir_all(output_dir)?;

    // scan for existing mapsets
    println!("Scanning directory: {}", output_dir.display());
    let existing_mapsets: HashSet<u32> = fs::read_dir(output_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "osz"))
        .filter_map(|e| {
            // check if the file size is correct
            if e.metadata().map(|m| m.len() == 0).unwrap_or(true) {
                return None;
            }
            // parse ID from start of filename
            e.file_name().to_str().and_then(|s| {
                s.split_whitespace().next().and_then(|id| id.parse::<u32>().ok())
            })
        })
        .collect();

    let missing_maps: Vec<&BeatmapInfo> = maps
        .iter()
        .filter(|m| !existing_mapsets.contains(&m.beatmapset_id))
        .collect();

    println!("Total maps:        {}", maps.len());
    println!("Already downloaded: {}", existing_mapsets.len());
    println!("To download:       {}\n", missing_maps.len());

    if missing_maps.is_empty() {
        println!("All maps up to date!");
        return Ok(());
    }

    let client = Client::builder()
        .user_agent("osu-beatmap-downloader/1.0.0 (https://github.com/zfi2/osu-beatmap-downloader)")
        .timeout(Duration::from_secs(120))
        .build()?;

    let rate_limiter = Arc::new(RateLimiter::new(mirror, client.clone()));
    if mirror == Mirror::Catboy {
        rate_limiter.refresh_catboy_limits().await?;
    }

    let multi_progress = MultiProgress::new();
    let overall_pb = multi_progress.add(ProgressBar::new(missing_maps.len() as u64));
    overall_pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let status_pb = multi_progress.add(ProgressBar::new(0));
    status_pb.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());

    let max_concurrent = match mirror {
        Mirror::Catboy => 1,
        Mirror::Nerinyan => 3,
    };

    let downloads = stream::iter(missing_maps)
        .map(|beatmap| {
            let client = &client;
            let output_dir = &output_dir;
            let rate_limiter = &rate_limiter;
            let status_pb = &status_pb;
            let overall_pb = &overall_pb;

            async move {
                // add some jitter for good measure
                let jitter = rand::random::<u64>() % 500;
                tokio::time::sleep(Duration::from_millis(jitter)).await;

                match download_beatmap(client, beatmap, output_dir, mirror, rate_limiter, status_pb).await {
                    Ok(_) => {
                        overall_pb.inc(1);
                    }
                    Err(e) => {
                        status_pb.println(format!("Failed to download {}: {}", beatmap.beatmapset_id, e));
                    }
                }
            }
        })
        .buffer_unordered(max_concurrent);

    //execute the stream
    downloads.collect::<Vec<()>>().await;

    overall_pb.finish_with_message("All downloads complete!");
    status_pb.finish_and_clear();

    println!("\nDone! Check {}", output_dir.display());
    Ok(())
}