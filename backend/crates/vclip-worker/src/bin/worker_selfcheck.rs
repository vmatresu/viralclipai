use std::path::Path;
use std::process::Command;

use vclip_worker::WorkerConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = WorkerConfig::from_env();

    println!(
        "worker-selfcheck: starting with work_dir={}",
        config.work_dir
    );
    ensure_workdir(&config.work_dir).await?;
    ensure_ffmpeg()?;
    ensure_env_present(&["REDIS_URL"])?;

    println!("worker-selfcheck: ok");
    Ok(())
}

async fn ensure_workdir<P: AsRef<Path>>(path: P) -> anyhow::Result<()> {
    let path = path.as_ref();
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

fn ensure_ffmpeg() -> anyhow::Result<()> {
    let output = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map_err(|e| anyhow::anyhow!("ffmpeg not available: {}", e))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "ffmpeg -version failed: {:?}",
            output.status
        ));
    }
    Ok(())
}

fn ensure_env_present(vars: &[&str]) -> anyhow::Result<()> {
    for var in vars {
        if std::env::var(var).is_err() {
            return Err(anyhow::anyhow!("missing required env var {}", var));
        }
    }
    Ok(())
}
