# osu! beatmap downloader

a command-line tool to fetch and download all your most played osu! beatmaps (which in theory, includes all the beatmaps you've ever played).
useful for new osu! installs

## features

- fetches your complete most played beatmap list from the osu! API
- downloads beatmaps from nerinyan.moe or catboy.best mirrors
- resumes downloads (skips already downloaded beatmaps)
- exports your beatmaps to a JSON format
- optional configurable parameters via `.env` environment variables

## prerequisites

- rust toolchain (install from https://rustup.rs/)
- osu! API credentials (see setup section below)

## setup

1. clone or download this repository

2. get your osu! API credentials:
   - visit https://osu.ppy.sh/home/account/edit#oauth
   - click on "New OAuth Application"
   - put in any name you wish
   - copy your client ID and client secret

3. Create a `.env` file in the project root

4. Edit `.env` and fill in your credentials:
   ```env
   # osu! API credentials
   OSU_CLIENT_ID=your_client_id_here
   OSU_CLIENT_SECRET=your_client_secret_here
   OSU_USERNAME=your_osu_username
   
   # optional: beatmap download directory (defaults to ./beatmaps)
   BEATMAP_OUTPUT_DIR=./beatmaps

   # optional: use alternative mirror (catboy.best instead of nerinyan.moe)
   # set to 'true' or 'yes' to enable (defaults to false)
   USE_ALTERNATIVE_MIRROR=false
   ```

## usage

### fetch and download in one command:
```bash
cargo run --release -- all
```

### or run separately:

fetch beatmap list:
```bash
cargo run --release -- fetch
```

download beatmaps:
```bash
cargo run --release -- download
```

## output files

- `osu_most_played_maps.json` - full beatmap information in a JSON format
- `beatmaps/*.osz` - downloaded beatmap files (ready to import into osu!)

files are saved in this format: `{beatmapset_id} {artist} - {title}.osz`

## how it works

1. **fetching**: authenticates with the osu! API and retrieves your complete most played beatmap list (with a silly progress indicator)
2. **re-fetching**: when using the `all` command, if a beatmap list already exists, you'll be prompted whether to re-fetch or use the existing data
3. **download**: uses the nerinyan.moe and catboy.best mirror API's to download beatmap files
4. **rate limiting**: automatically adapts to beatmap mirrors rate limits
5. **resume**: skips already downloaded files, making it safe to re-run

## troubleshooting

1. **authentication failed**: double-check your client ID and client secret in your `.env` file
2. **missing dependencies**: run `cargo build` to install all required dependencies
3. **rate limited**: the tool should handle this automatically, if not, you just have to wait and re-run the tool later

## license

this project is licensed under the MIT License. see the [LICENSE](LICENSE) file for details.
