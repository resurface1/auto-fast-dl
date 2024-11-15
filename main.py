import aiohttp
import asyncio
import aiofiles
import os
import uuid
import psutil
import time
import sys
import signal
from tqdm import tqdm
from typing import Optional
from dataclasses import dataclass
import gc

VERSION = "3.0.0"

def print_banner():
    banner = """
    ╔═══════════════════════════════════════════════════════════════╗
    ║                                                               ║
    ║     █████╗ ██╗   ██╗████████╗ ██████╗      ███████╗██████╗   ║
    ║    ██╔══██╗██║   ██║╚══██╔══╝██╔═══██╗     ██╔════╝██╔══██╗  ║
    ║    ███████║██║   ██║   ██║   ██║   ██║     █████╗  ██║  ██║  ║
    ║    ██╔══██║██║   ██║   ██║   ██║   ██║     ██╔══╝  ██║  ██║  ║
    ║    ██║  ██║╚██████╔╝   ██║   ╚██████╔╝     ██║     ██████╔╝  ║
    ║    ╚═╝  ╚═╝ ╚═════╝    ╚═╝    ╚═════╝      ╚═╝     ╚═════╝   ║
    ║                                                               ║
    ║     ██████╗ ██╗         ██████╗  ██████╗ ████████╗          ║
    ║     ██╔══██╗██║         ██╔══██╗██╔═══██╗╚══██╔══╝          ║
    ║     ██║  ██║██║         ██████╔╝██║   ██║   ██║             ║
    ║     ██║  ██║██║         ██╔══██╗██║   ██║   ██║             ║
    ║     ██████╔╝███████╗    ██████╔╝╚██████╔╝   ██║             ║
    ║     ╚═════╝ ╚══════╝    ╚═════╝  ╚═════╝    ╚═╝             ║
    ║                                                               ║
    ╠═══════════════════════════════════════════════════════════════╣
    ║  Version: {:<52} ║
    ║  GitHub: https://github.com/yukisakuna/auto-fast-dl          ║
    ╚═══════════════════════════════════════════════════════════════╝
    """.format(VERSION)
    
    print("\033[36m" + banner + "\033[0m")
    
    loading = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    print("\033[35m", end="")
    for _ in range(2):
        for char in loading:
            sys.stdout.write(f'\r        System Initializing... {char}')
            sys.stdout.flush()
            time.sleep(0.1)
    print("\033[0m\n")

    print("\033[33m╔════ System Information ════╗\033[0m")
    print(f"\033[33m║\033[0m CPU Cores: {os.cpu_count()}")
    print(f"\033[33m║\033[0m Memory Available: {psutil.virtual_memory().available / (1024**3):.1f} GB")
    print(f"\033[33m║\033[0m Operating System: {sys.platform}")
    print("\033[33m╚══════════════════════════╝\033[0m\n")

@dataclass
class DownloadStats:
    total_files: int = 0
    total_bytes: int = 0
    start_time: Optional[float] = None
    
import os
import gc
import uuid
import time
import psutil
import asyncio
import aiohttp
import aiofiles
from tqdm import tqdm

class DownloadStats:
    def __init__(self):
        self.start_time = 0
        self.total_bytes = 0
        self.total_files = 0

class DownloadManager:
    def __init__(self, download_dir: str = "downloads", max_memory_mb: int = 300):
        self.download_dir = download_dir
        self.max_memory_mb = max_memory_mb
        self.stats = DownloadStats()
        self.last_end_time = None
        self.setup_download_dir()
        
    def setup_download_dir(self):
        if not os.path.exists(self.download_dir):
            os.makedirs(self.download_dir)
            
    def check_memory_availability(self, batch_size: int, estimated_file_size_mb: float) -> bool:
        available_memory_mb = psutil.virtual_memory().available / (1024 * 1024)
        required_memory_mb = batch_size * estimated_file_size_mb
        
        print(f"\nMemory Check:")
        print(f"╔════ Memory Analysis ════╗")
        print(f"║ Available Memory: {available_memory_mb:.1f} MB")
        print(f"║ Required Memory: {required_memory_mb} MB")
        print(f"║ Batch Size: {batch_size}")
        print(f"║ Est. File Size: {estimated_file_size_mb:.1f} MB")
        print(f"╚═══════════════════════╝\n")
        
        return available_memory_mb >= required_memory_mb

    def _get_memory_usage_mb(self) -> float:
        return psutil.Process(os.getpid()).memory_info().rss / 1024 / 1024

    async def get_file_size(self, session: aiohttp.ClientSession, url: str) -> int:
        try:
            async with session.head(url) as response:
                if response.status == 200:
                    return int(response.headers.get('content-length', 0))
                else:
                    async with session.get(url) as response:
                        if response.status == 200:
                            return int(response.headers.get('content-length', 0))
        except Exception as e:
            print(f"Error getting file size: {str(e)}")
        return 0
    
    async def _save_to_disk(self, content: bytes, file_path: str):
        async with aiofiles.open(file_path, 'wb') as f:
            await f.write(content)
            
    async def download_file(self, session: aiohttp.ClientSession, url: str, 
                          file_path: str, progress_bar: tqdm) -> bool:
        try:
            async with session.get(url) as response:
                if response.status != 200:
                    print(f"Failed to download {url}, status code: {response.status}")
                    return False
                
                content = await response.read()
                memory_usage = self._get_memory_usage_mb()
                
                if memory_usage + (len(content) / 1024 / 1024) < self.max_memory_mb:
                    self.stats.total_bytes += len(content)
                else:
                    await self._save_to_disk(content, file_path)
                    self.stats.total_bytes += len(content)
                
                del content
                gc.collect()
                progress_bar.update(1)
                return True
                
        except Exception as e:
            print(f"Error downloading {url}: {str(e)}")
            return False
        
    def cleanup_files(self):
        for file_name in os.listdir(self.download_dir):
            file_path = os.path.join(self.download_dir, file_name)
            try:
                if os.path.exists(file_path):
                    os.remove(file_path)
            except Exception as e:
                print(f"Error cleaning up {file_path}: {str(e)}")
                
    async def start(self, url: str, batch_size: int = 20):
        if not url.startswith(('http://', 'https://')):
            print("Invalid URL. Please provide a URL that starts with 'http://' or 'https://'.")
            return
        
        # 最初にファイルサイズを取得
        async with aiohttp.ClientSession() as session:
            file_size = await self.get_file_size(session, url)
            file_size_mb = file_size / (1024 * 1024)
        
        # 利用可能メモリに基づいてバッチサイズを調整
        available_memory_mb = psutil.virtual_memory().available / (1024 * 1024)
        safe_batch_size = max(1, int(available_memory_mb / (file_size_mb * 2)))  # 2倍の安全マージン
        actual_batch_size = min(batch_size, safe_batch_size)
        
        print(f"\nAdjusted batch size to {actual_batch_size} based on available memory")
        
        # メモリチェック（実際のファイルサイズを使用）
        if not self.check_memory_availability(actual_batch_size, estimated_file_size_mb=file_size_mb):
            print("Warning: Running in disk-based mode with reduced batch size")
            self.max_memory_mb = 0
        
        self.stats.start_time = time.time()
        
        async with aiohttp.ClientSession(connector=aiohttp.TCPConnector(limit=actual_batch_size)) as session:
            while True:
                batch_start_time = time.time()
                tasks = []
                with tqdm(total=actual_batch_size, desc="Downloading files") as progress_bar:
                    for _ in range(actual_batch_size):
                        file_name = f"{uuid.uuid4()}.dat"
                        file_path = os.path.join(self.download_dir, file_name)
                        tasks.append(self.download_file(session, url, file_path, progress_bar))
                    
                    results = await asyncio.gather(*tasks, return_exceptions=True)
                    successful_downloads = sum(1 for r in results if r is True)
                    self.stats.total_files += successful_downloads
                
                current_time = time.time()
                if self.last_end_time is None:
                    elapsed_time = current_time - batch_start_time
                else:
                    elapsed_time = current_time - self.last_end_time
                
                self.last_end_time = current_time
                avg_speed = actual_batch_size / elapsed_time if elapsed_time > 0 else 0
                
                print(f"\n{actual_batch_size} files downloaded in {elapsed_time:.2f} seconds, "
                      f"average speed: {avg_speed:.2f} files/second")
                
                self.cleanup_files()
                gc.collect()
                await asyncio.sleep(1)

def handle_exit(signum, frame, download_manager: DownloadManager):
    print("\nComplete!")
    print(f"Total files downloaded: {download_manager.stats.total_files}")
    print(f"Total data downloaded: {download_manager.stats.total_bytes / (1024 * 1024 * 1024):.2f} GB")
    download_manager.cleanup_files()
    exit(0)

if __name__ == "__main__":
    print_banner()
    download_manager = DownloadManager()
    signal.signal(signal.SIGINT, 
                 lambda s, f: handle_exit(s, f, download_manager))
    
    url = input("Enter the URL to download: ")
    asyncio.run(download_manager.start(url))
