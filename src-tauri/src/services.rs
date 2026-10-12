pub mod aria2c;
pub mod ffmpeg;
pub mod login;
pub mod queue;
pub mod local_api;

use crate::shared::process_err as err;

pub async fn init() -> anyhow::Result<()> {
    aria2c::init().await.map_err(|e| err(e, "aria2c"))?;
    ffmpeg::test().await.map_err(|e| err(e, "ffmpeg"))?;
    Ok(())
}
