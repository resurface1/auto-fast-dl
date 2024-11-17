use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicI64, AtomicU64, Ordering},
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use num_format::{Locale, ToFormattedString};
use sysinfo::{Pid, System};
use tokio::{fs::File, io::AsyncWriteExt, io::BufWriter, sync::Mutex};
use uuid::Uuid;

const VERSION: &str = "3.1.0r";

#[derive(Debug, Default)]
struct DownloadStats {
    total_files: usize,
    failed_downloads: usize,
    total_bytes: u64,
    start_time: Option<u64>,
}

struct Downloader {
    download_dir: String,
    max_memory_mb: AtomicU64,
    stats: Arc<Mutex<DownloadStats>>,
    last_end_time: AtomicI64,
}

impl Downloader {
    fn new(download_dir: Option<String>, max_memory_mb: Option<u64>) -> Self {
        let this = Downloader {
            download_dir: download_dir.unwrap_or_else(|| "downloads".to_string()),
            max_memory_mb: AtomicU64::new(max_memory_mb.unwrap_or(300)),
            stats: Arc::new(Mutex::new(DownloadStats::default())),
            last_end_time: AtomicI64::new(-1),
        };
        this.setup_download_dir();
        this
    }

    fn setup_download_dir(&self) {
        if !Path::new(&self.download_dir).exists() {
            fs::create_dir_all(&self.download_dir).expect("Failed to create download directory");
        }
    }

    /// Cleanup files in the download directory
    pub fn cleanup_files(&self) {
        let files = fs::read_dir(&self.download_dir).expect("Failed to read download directory");
        for file in files {
            let file = file.expect("Failed to read file");
            let path = file.path();
            if path.is_file() {
                fs::remove_file(path).expect("Failed to remove file");
            }
        }
    }

    pub fn check_memory_availability(
        &self,
        system: &System,
        batch_size: usize,
        estimated_file_size_mb: f64,
    ) -> bool {
        let available_memory_mb = (system.available_memory() as f64) / 1024.0 / 1024.0;
        let required_memory_mb = batch_size as f64 * estimated_file_size_mb;
        println!("\nMemory Check:");
        println!("╔═══════ Memory Analysis ═══════╗");
        println!("║ Available Memory: {:>8.1} MB ║", available_memory_mb);
        println!("║ Required Memory: {:>9.1} MB ║", required_memory_mb);
        println!("║ Batch Size: {:>17} ║", batch_size);
        println!("║ Est. File Size: {:>10.1} MB ║", estimated_file_size_mb);
        println!("╚═══════════════════════════════╝\n");
        available_memory_mb > required_memory_mb
    }

    fn get_memory_usage_mb(&self, system: &System) -> f64 {
        let process = system.process(Pid::from_u32(std::process::id())).unwrap();
        (process.memory() as f64) / 1024.0 / 1024.0
    }

    async fn get_file_size(&self, client: &reqwest::Client, url: &str) -> anyhow::Result<u64> {
        let response = client.head(url).send().await?;
        let headers = response.headers();
        let content_length = headers
            .get("Content-Length")
            .ok_or(anyhow::anyhow!("Content-Length not provided"))?
            .to_str()?;
        content_length
            .parse::<u64>()
            .map_err(|e| anyhow::anyhow!("Failed to parse Content-Length: {}", e))
    }

    async fn save_to_disk(&self, content: &[u8], file_name: &str) -> anyhow::Result<()> {
        let file_path = format!("{}/{}", self.download_dir, file_name);
        let file = File::create(file_path).await?;
        let mut writer = BufWriter::new(file);
        writer.write_all(content).await?;
        writer.flush().await?;
        Ok(())
    }

    pub async fn download_file(
        &self,
        client: &reqwest::Client,
        system: &System,
        url: &str,
        file_path: impl Into<String>,
        bar: ProgressBar,
    ) -> anyhow::Result<()> {
        let file_path = file_path.into();
        let response = match client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("Failed to download {}: {}", url, e);
                let mut lock = self.stats.lock().await;
                lock.failed_downloads += 1;
                return Err(anyhow::anyhow!("Failed to download file"));
            }
        };
        if !response.status().is_success() {
            eprintln!(
                "Failed to download {url}, status code: {}",
                response.status().as_str()
            );
            let mut lock = self.stats.lock().await;
            lock.failed_downloads += 1;
            return Err(anyhow::anyhow!("Failed to download file"));
        }

        let content = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Failed to read content from {}: {}", url, e);
                let mut lock = self.stats.lock().await;
                lock.failed_downloads += 1;
                return Err(anyhow::anyhow!("Failed to read content"));
            }
        };

        let content_size_mb = content.len() as f64 / 1024.0 / 1024.0;
        let memory_usage_mb = self.get_memory_usage_mb(system);

        if memory_usage_mb + content_size_mb < self.max_memory_mb.load(Ordering::Relaxed) as f64 {
            let mut lock = self.stats.lock().await;
            lock.total_bytes += content.len() as u64;
        } else {
            if let Err(e) = self.save_to_disk(&content, &file_path).await {
                eprintln!("Failed to save {}: {}", file_path, e);
                let mut lock = self.stats.lock().await;
                lock.failed_downloads += 1;
                return Err(anyhow::anyhow!("Failed to save file"));
            }
            let mut lock = self.stats.lock().await;
            lock.total_bytes += content.len() as u64;
        }

        drop(content);

        bar.inc(1);

        Ok(())
    }

    pub async fn display_completion_banner(&self) {
        let lock = self.stats.lock().await;
        let gb_downloaded = lock.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let total_time = Utc::now().timestamp() as u64 - lock.start_time.unwrap_or(0);
        let completion_banner = format!(
            "╔══════════════════ Download Complete ══════════════════╗
║                                                       ║
║  📊 Statistics:                                       ║
║  ├─ Total Files: {:<35}  ║
║  ├─ Failed Downloads: {:<30}  ║
║  ├─ Data Downloaded: {:<30}   ║
║  └─ Total Time: {:<30}        ║
║                                                       ║
║  🎉 Download Session Completed Successfully! 🎉       ║
║                                                       ║
╚═══════════════════════════════════════════════════════╝",
            lock.total_files.to_formatted_string(&Locale::en),
            lock.failed_downloads.to_formatted_string(&Locale::en),
            format!("{:.2} GB", gb_downloaded),
            format!("{:.2} seconds", total_time)
        );
        println!("{}", completion_banner.green());
    }

    pub async fn start(&self, url: &str, batch_size: Option<usize>) -> anyhow::Result<()> {
        let batch_size = batch_size.unwrap_or(20);
        if !url.starts_with("http://") && !url.starts_with("https://") {
            eprintln!(
                "Invalid URL. Please provide a URL that starts with 'http://' or 'https://'."
            );
            return Err(anyhow::anyhow!("Invalid URL"));
        }

        let client = reqwest::Client::new();
        let file_size = self.get_file_size(&client, url).await?;
        let file_size_mb = file_size as f64 / 1024.0 / 1024.0;

        let mut system = System::new_all();
        system.refresh_all();
        let available_memory_mb = system.available_memory() as f64 / 1024.0 / 1024.0;
        let safe_batch_size = std::cmp::max(1, (available_memory_mb / file_size_mb * 2.0) as usize);
        let actual_batch_size = std::cmp::min(batch_size, safe_batch_size);

        println!("\nAdjusted batch size to {actual_batch_size} based on available memory");

        if !self.check_memory_availability(&system, actual_batch_size, file_size_mb) {
            eprintln!("Warning: Running in disk-based mode with reduced batch size");
            self.max_memory_mb.store(0, Ordering::Relaxed);
        }

        let client = reqwest::ClientBuilder::new()
            .pool_max_idle_per_host(actual_batch_size)
            .timeout(Duration::from_secs(30))
            .build()?;

        let download_dir = self.download_dir.clone();
        let mut lock = self.stats.lock().await;
        lock.start_time = Some(Utc::now().timestamp() as u64);
        drop(lock);

        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    println!("Ctrl+C detected! Exiting loop...");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let batch_start_time = Utc::now().timestamp() as u64;
                    let mut tasks = Vec::with_capacity(actual_batch_size);
                    let bar = ProgressBar::new(actual_batch_size as u64);
                    bar.set_style(
                        ProgressStyle::default_bar()
                            .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})")
                            .unwrap()
                            .progress_chars("#>-"),
                    );

                    bar.tick();

                    for _ in 0..actual_batch_size {
                        let file_name = format!("{}.dat", Uuid::new_v4());
                        let file_path = Path::new(&download_dir).join(file_name);
                        let file_path = file_path.to_str().unwrap().to_string();
                        let d = self.download_file(&client, &system, url, file_path, bar.clone());
                        tasks.push(d);
                    }

                    let results = futures::future::join_all(tasks).await;

                    let successful_downloads = results.iter().filter(|&result| result.is_ok()).count();
                    let mut lock = self.stats.lock().await;
                    lock.total_files += successful_downloads;
                    drop(lock);

                    bar.finish();

                    let current_time = Utc::now().timestamp() as u64;
                    let last_end_time = self.last_end_time.load(Ordering::Relaxed);
                    let elapsed_time = if last_end_time >= 0 {
                        current_time - last_end_time as u64
                    } else {
                        current_time - batch_start_time
                    };

                    self.last_end_time.store(current_time as i64, Ordering::Relaxed);
                    let avg_speed = actual_batch_size as f64 / (if elapsed_time > 0 { elapsed_time as f64 } else { 1.0 });

                    println!("\n{actual_batch_size} files downloaded in {elapsed_time:.2} seconds, ");
                    println!("average speed: {avg_speed:.2} files/second");

                    self.cleanup_files();
                }
            }
        }

        handle_exit(self).await;

        Ok(())
    }
}

async fn handle_exit(downloader: &Downloader) {
    println!("\nComplete!");
    let s = downloader.stats.lock().await;
    println!("Total files downloaded: {}", s.total_files);
    println!(
        "Total data downloaded: {:.2} GB",
        s.total_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    );
    drop(s);
    downloader.cleanup_files();
    downloader.display_completion_banner().await;
    std::process::exit(0);
}

#[inline]
fn print_banner() {
    let banner = format!(
        "
    ╔═══════════════════════════════════════════════════════════════╗
    ║                                                               ║
    ║     █████╗ ██╗   ██╗████████╗ ██████╗      ███████╗██████╗    ║
    ║    ██╔══██╗██║   ██║╚══██╔══╝██╔═══██╗     ██╔════╝██╔══██╗   ║
    ║    ███████║██║   ██║   ██║   ██║   ██║     █████╗  ██║  ██║   ║
    ║    ██╔══██║██║   ██║   ██║   ██║   ██║     ██╔══╝  ██║  ██║   ║
    ║    ██║  ██║╚██████╔╝   ██║   ╚██████╔╝     ██║     ██████╔╝   ║
    ║    ╚═╝  ╚═╝ ╚═════╝    ╚═╝    ╚═════╝      ╚═╝     ╚═════╝    ║
    ║                                                               ║
    ║     ██████╗ ██╗         ██████╗  ██████╗ ████████╗            ║
    ║     ██╔══██╗██║         ██╔══██╗██╔═══██╗╚══██╔══╝            ║
    ║     ██║  ██║██║         ██████╔╝██║   ██║   ██║               ║
    ║     ██║  ██║██║         ██╔══██╗██║   ██║   ██║               ║
    ║     ██████╔╝███████╗    ██████╔╝╚██████╔╝   ██║               ║
    ║     ╚═════╝ ╚══════╝    ╚═════╝  ╚═════╝    ╚═╝               ║
    ║                                                               ║
    ╠═══════════════════════════════════════════════════════════════╣
    ║  Version: {:<51} ║
    ║  GitHub: https://github.com/resurface1/auto-fast-dl           ║
    ╚═══════════════════════════════════════════════════════════════╝
    ",
        VERSION
    );

    println!("{}", banner.cyan());

    let mut system = System::new_all();
    system.refresh_all();

    let cpu_cores = system.cpus().len();

    let memory_available = (system.available_memory() as f64) / 1024.0 / 1024.0 / 1024.0;

    let operating_system = std::env::consts::OS;

    let text = format!(
        "╔════ System Information ════╗
║ CPU Cores: {:<9}       ║
║ Memory Available: {:<5.2} GB ║
║ Operating System: {:<8} ║
╚════════════════════════════╝",
        cpu_cores, memory_available, operating_system
    );

    println!("{}", text.yellow());
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_banner();

    let downloader = Downloader::new(None, None);

    print!("Enter the URL to download: ");
    io::stdout().flush()?;
    let mut url = String::new();
    io::stdin().read_line(&mut url)?;
    let url = url.trim();

    downloader.start(url, None).await?;
    Ok(())
}
