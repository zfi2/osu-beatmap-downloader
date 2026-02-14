use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapInfo {
    pub beatmap_id: u32,
    pub beatmapset_id: u32,
    pub title: String,
    pub artist: String,
    pub version: String,
    pub play_count: u32,
    pub download_link: String,
}

impl BeatmapInfo {
    pub fn filename(&self) -> String {
        let artist = sanitize_filename(&self.artist);
        let title = sanitize_filename(&self.title);
        format!("{} {} - {}.osz", self.beatmapset_id, artist, title)
    }
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim_matches(|c| c == '.' || c == ' ')
        .to_string()
}