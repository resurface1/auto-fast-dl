import aiohttp
import asyncio
import aiofiles
import os
import uuid
from tqdm import tqdm
import time
import signal
from fake_useragent import UserAgent


DOWNLOAD_DIR = "downloads"


if not os.path.exists(DOWNLOAD_DIR):
    os.makedirs(DOWNLOAD_DIR)

total_files_downloaded = 0
total_bytes_downloaded = 0


ua = UserAgent()

async def download_file(session, url, file_path, progress_bar, retry_count=3):
    global total_bytes_downloaded
    try:
        headers = {'User-Agent': ua.random}
        async with session.get(url, headers=headers) as response:
            if response.status == 200:
                async with aiofiles.open(file_path, 'wb') as f:
                    async for chunk in response.content.iter_chunked(8192):
                        await f.write(chunk)
                        total_bytes_downloaded += len(chunk)
                progress_bar.update(1)
            else:
                print(f"Failed to download {url}, status code: {response.status}")
    except Exception as e:
        if retry_count > 0:
            print(f"Error downloading {url}. Retrying... ({3 - retry_count + 1})")
            await asyncio.sleep(1)
            await download_file(session, url, file_path, progress_bar, retry_count - 1)
        else:
            print(f"Failed to download {url} after retries: {e}")

async def main(url):
    global total_files_downloaded
    # URLのバリデーション
    if not url.startswith(('http://', 'https://')):
        print("Invalid URL. Please provide a URL that starts with 'http://' or 'https://'.")
        return

    conn = aiohttp.TCPConnector(limit_per_host=50) 
    timeout = aiohttp.ClientTimeout(total=30)  

    while True:
        start_time = time.time()
        async with aiohttp.ClientSession(connector=conn, timeout=timeout) as session:
            tasks = []
            with tqdm(total=20, desc="Downloading files") as progress_bar:
                for _ in range(20): 
                    file_name = f"{uuid.uuid4()}.dat" 
                    file_path = os.path.join(DOWNLOAD_DIR, file_name)
                    tasks.append(download_file(session, url, file_path, progress_bar))

                await asyncio.gather(*tasks)

        end_time = time.time()
        elapsed_time = end_time - start_time
        average_speed = 20 / elapsed_time if elapsed_time > 0 else 0
        print(f"20 files downloaded in {elapsed_time:.2f} seconds, average speed: {average_speed:.2f} files/second")

        total_files_downloaded += 20
        print(f"Total files downloaded so far: {total_files_downloaded}")
        print(f"Total data downloaded so far: {total_bytes_downloaded / (1024 * 1024):.2f} MB")

        for file_name in os.listdir(DOWNLOAD_DIR):
            file_path = os.path.join(DOWNLOAD_DIR, file_name)
            if os.path.exists(file_path):
                os.remove(file_path)

def handle_exit(signum, frame):
    global total_files_downloaded, total_bytes_downloaded
    print("\nComplete!")
    print(f"Total files downloaded: {total_files_downloaded}")
    print(f"Total data downloaded: {total_bytes_downloaded / (1024 * 1024 * 1024):.2f} GB")
    exit(0)

if __name__ == "__main__":
    signal.signal(signal.SIGINT, handle_exit)
    url = input("Enter the URL to download: ")
    asyncio.run(main(url))
