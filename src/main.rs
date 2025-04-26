// 1. Bring in necessary crates and traits.
use reqwest::Client;                                  // Async HTTP client
use serde::Deserialize;                               // Derive Deserialize for JSON mapping
use std::env;                                         // For command-line args
use std::fs::{self, File};
use std::io::Write;                                   // For writing to files
use std::os::unix::fs::PermissionsExt;                // For setting Unix permission bits
use std::path::{Path, PathBuf};
use std::process::Command;                            // For running shell commands
use indicatif::{ProgressBar, ProgressStyle};          // For progress bar
use futures_util::StreamExt;                          // For stream handling

// 2. Define structs matching the JSON structure from GitHub API.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    // TODO: Temporary default install directory
    let mut install_dir = PathBuf::from(env::var("HOME")?).join("./Documents/repository/rust-unicorn");
    let mut create_symlink = true;
    let mut force_update = false;
    let mut quiet = false;

    // Simple command-line argument parsing
    for i in 1..args.len() {
        match args[i].as_str() {
            "--install-dir" | "-d" if i + 1 < args.len() => {
                install_dir = PathBuf::from(&args[i + 1]);
            }
            "--no-symlink" => {
                create_symlink = false;
            }
            "--force" | "-f" => {
                force_update = true;
            }
            "--quiet" | "-q" => {
                quiet = true;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            _ => {}
        }
    }

    // Create installation directory if it doesn't exist
    if !install_dir.exists() {
        fs::create_dir_all(&install_dir)?;
        if !quiet {
            println!("Created directory: {}", install_dir.display());
        }
    }

    // 3. Configure owner/repo and construct the "latest release" API URL.
    let owner = "laurent22";
    let repo = "joplin";
    let api_url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        owner, repo
    );

    // 4. Create an HTTP client with a User-Agent header to satisfy GitHub's requirements.
    let client = Client::builder()
        .user_agent("rust-joplin-installer")
        .build()?;

    // 5. Fetch and deserialize the release information.
    let release: Release = client
        .get(&api_url)
        .send()
        .await?
        .json()
        .await?;

    // 6. Find the first asset whose name ends with ".AppImage".
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name.ends_with(".AppImage"))
        .ok_or("No AppImage asset found in latest release")?;

    let install_path = install_dir.join(&asset.name);
    
    // Check if we already have the latest version
    if install_path.exists() && !force_update {
        if !quiet {
            println!("Joplin {} is already installed at {}", release.tag_name, install_path.display());
            println!("Use --force to reinstall or update.");
        }
        
        // Make sure it's executable anyway
        let mut perms = fs::metadata(&install_path)?.permissions();
        perms.set_mode(perms.mode() | 0o755);
        fs::set_permissions(&install_path, perms)?;
        
        // Set up symlink if needed
        if create_symlink {
            create_joplin_symlink(&install_dir, &asset.name)?;
        }
        
        return Ok(());
    }

    if !quiet {
        println!("Found Joplin {} ({})", release.tag_name, asset.name);
        println!("Downloading to {}...", install_path.display());
    }

    // 7. Download the binary asset with progress bar.
    let resp = client
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()?;
    
    // Get the content length for the progress bar
    let total_size = resp.content_length().unwrap_or(0);
    
    // Create and configure the progress bar
    let pb = if !quiet && total_size > 0 {
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"));
        pb
    } else {
        ProgressBar::hidden()
    };

    // Stream the download with progress updates
    let mut file = File::create(&install_path)?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }
    
    pb.finish_with_message("Download complete!");

    // 9. Update file permissions to add the executable bit (chmod +x).
    let mut perms = file.metadata()?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(&install_path, perms)?;

    if !quiet {
        println!("Downloaded and made executable: {}", install_path.display());
    }
    
    // 10. Create a symlink for easier access
    if create_symlink {
        create_joplin_symlink(&install_dir, &asset.name)?;
    }
    
    if !quiet {
        println!("Joplin {} has been successfully installed!", release.tag_name);
        println!("You can run it by typing 'joplin' in your terminal.");
    }
    
    Ok(())
}

fn create_joplin_symlink(install_dir: &Path, app_image_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let symlink_path = install_dir.join("joplin");
    
    // Remove existing symlink if it exists
    if symlink_path.exists() {
        fs::remove_file(&symlink_path)?;
    }
    
    // Create the symlink
    std::os::unix::fs::symlink(app_image_name, &symlink_path)?;
    
    Ok(())
}

fn print_help() {
    println!("Joplin AppImage Installer");
    println!();
    println!("USAGE:");
    println!("    rust-unicorn [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -d, --install-dir <PATH>    Installation directory (default: ~/Documents/repository/rust-unicorn)");
    println!("    --no-symlink                Don't create a 'joplin' symlink");
    println!("    -f, --force                 Force download even if already installed");
    println!("    -q, --quiet                 Suppress output messages");
    println!("    -h, --help                  Print this help message");
}
