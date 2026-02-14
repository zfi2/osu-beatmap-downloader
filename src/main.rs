use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;

mod downloader;
mod fetcher;
mod types;

#[derive(Parser)]
#[command(name = "osu-beatmap-backup")]
#[command(about = "Fetch and download your osu! most played beatmaps", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// fetch most played beatmaps from the osu! API
    Fetch {
        /// output JSON file path
        #[arg(short, long, default_value = "osu_most_played_maps.json")]
        output: PathBuf,
    },
    /// download beatmaps from the JSON file
    Download {
        /// input JSON file path
        #[arg(short, long, default_value = "osu_most_played_maps.json")]
        input: PathBuf,
        /// output directory for beatmaps
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// fetch and download in one command
    All {
        /// output directory for beatmaps
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn get_default_output_dir() -> PathBuf {
    std::env::var("BEATMAP_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("beatmaps"))
}

fn prompt_confirm(msg: &str) -> Result<bool> {
    print!("{} (y/N): ", msg);
    std::io::stdout().flush()?;
    
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    let cli = Cli::parse();

    match cli.command {
        Commands::Fetch { output } => {
            println!("Fetching beatmaps from osu! API...");
            let maps = fetcher::fetch_most_played().await?;
            fetcher::save_beatmaps(&maps, &output)?;
            println!("Saved {} beatmaps to {}", maps.len(), output.display());
        }
        Commands::Download { input, output } => {
            println!("Loading beatmaps from {}...", input.display());
            let maps = fetcher::load_beatmaps(&input)?;
            println!("Found {} beatmaps", maps.len());
            
            let output_dir = output.unwrap_or_else(get_default_output_dir);
            downloader::download_beatmaps(&maps, &output_dir).await?;
        }
        Commands::All { output } => {
            let json_path = PathBuf::from("osu_most_played_maps.json");
            let mut maps = Vec::new();

            if json_path.exists() {
                println!("Found existing beatmap list at {}", json_path.display());
                if prompt_confirm("Do you want to re-fetch from osu! API?")? {
                    maps = fetcher::fetch_most_played().await?;
                    fetcher::save_beatmaps(&maps, &json_path)?;
                    println!("Updated list saved to {}\n", json_path.display());
                } else {
                    println!("Using existing beatmap list...");
                    maps = fetcher::load_beatmaps(&json_path)?;
                }
            } else {
                maps = fetcher::fetch_most_played().await?;
                fetcher::save_beatmaps(&maps, &json_path)?;
                println!("Saved to {}\n", json_path.display());
            }
            
            let output_dir = output.unwrap_or_else(get_default_output_dir);
            downloader::download_beatmaps(&maps, &output_dir).await?;
        }
    }

    Ok(())
}