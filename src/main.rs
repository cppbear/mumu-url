use anyhow::{Context, bail};
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 参数解析
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        bail!(
            "Usage: {} <X> <Y> [download_dir]\nExample: {} 4.1.21.3664 0325 [./downloads]",
            args.get(0).unwrap_or(&"program".into()),
            args.get(0).unwrap_or(&"program".into())
        );
    }
    let x = args[1].clone();
    let y = args[2].clone();
    let download_dir = args.get(3).cloned();

    // 2. 生成Z值
    let z_values = generate_z_values();

    // 3. 创建同步工具
    let found = Arc::new(AtomicBool::new(false));
    let (sender, mut receiver) = mpsc::channel(1);

    // 4. 创建HTTP客户端
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to create HTTP client")?;

    // 5. 启动并发检测
    let client_clone = client.clone();
    let handles = tokio::spawn(async move {
        use futures::StreamExt;

        futures::stream::iter(z_values)
            .for_each_concurrent(1000, |z| {
                let client = client_clone.clone();
                let x = x.to_string();
                let y = y.to_string();
                let sender = sender.clone();
                let found = found.clone();

                async move {
                    if found.load(Ordering::Relaxed) {
                        return;
                    }

                    let url = format!(
                        "https://a11.gdl.netease.com/MuMuNG-setup-V{}-{}{}.exe",
                        x, y, z
                    );

                    if let Ok(resp) = client.head(&url).send().await {
                        if resp.status().is_success() {
                            let _ = sender.send(url).await;
                            found.store(true, Ordering::Relaxed);
                        }
                    }
                }
            })
            .await
    });

    // 6. 处理检测结果
    match tokio::time::timeout(Duration::from_secs(30), receiver.recv()).await {
        Ok(Some(url)) => {
            handles.abort();

            match download_dir {
                Some(dir) => {
                    println!("✅ Valid URL found: {}", url);
                    println!("Do you want to download? [y/n]");

                    let mut input = String::new();
                    std::io::stdin()
                        .read_line(&mut input)
                        .context("Failed to read input")?;

                    if input.trim().eq_ignore_ascii_case("y") {
                        download_file(&url, &dir).await?;
                    } else {
                        println!("Exiting without download.");
                    }
                }
                None => {
                    println!("{}", url);
                }
            }
            Ok(())
        }
        _ => {
            eprintln!("❌ No valid URL found within time limit");
            std::process::exit(1);
        }
    }
}

// 7. 下载函数
async fn download_file(url: &str, dir: &str) -> anyhow::Result<()> {
    // 创建目录
    tokio::fs::create_dir_all(dir).await?;

    // 获取文件名
    let file_name = url.split('/').last().context("Invalid URL format")?;
    let path = format!("{}/{}", dir, file_name);

    // 创建可调节的超时客户端
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600)) // 1小时超时
        .build()?;

    // 发起请求并获取响应流
    let response = client
        .get(url)
        .send()
        .await
        .context("Download request failed")?;

    // 创建文件并分块写入
    let mut file = tokio::fs::File::create(&path)
        .await
        .context("Failed to create file")?;

    let total_size = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    // 显示进度条
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{wide_bar}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("#>-"),
    );

    // 分块处理
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Download stream error")?;
        file.write_all(&chunk).await.context("Write chunk failed")?;

        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete");
    println!("🚀 Downloaded to: {}", path);
    Ok(())
}

// 8. Z值生成器
fn generate_z_values() -> Vec<String> {
    (0..24)
        .flat_map(|h| {
            (0..60).flat_map(move |m| (0..60).map(move |s| format!("{:02}{:02}{:02}", h, m, s)))
        })
        .collect()
}
