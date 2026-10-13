"""Compile the exact provider implementation without a desktop/WebKit host.
Only command/Specta annotations and host integrations are substituted. The
production parser, validation, command construction, subprocess, persistence
and cancellation implementations are included unchanged in the test crate.
"""
import pathlib,re,tempfile,subprocess,os,json,threading,http.server,socketserver,shutil,sys,importlib.util
root=pathlib.Path(__file__).resolve().parents[2]
source=(root/'src-tauri/src/providers/ytdlp.rs').read_text(encoding='utf-8')
source=re.sub(r'#\[(tauri::command\(async\)|specta::specta)\]\s*','',source)
source=source.replace('use specta::Type;','').replace('use tauri::{Emitter, Manager};','use crate::tauri_plugin_opener;')
source=re.sub(r',\s*Type\b','',source).replace('tauri::Url','url::Url').replace('tauri::async_runtime::spawn','tokio::spawn')
path=pathlib.Path(tempfile.mkdtemp(prefix='conest-provider-'))
(path/'src').mkdir();(path/'src/provider.rs').write_text(source,encoding='utf-8')
(path/'Cargo.toml').write_text('''[package]
name="conest-provider-headless"
version="0.1.0"
edition="2021"
[dependencies]
anyhow="1"
serde={version="1",features=["derive"]}
serde_json="1"
regex="1"
url="2"
tokio={version="1",features=["full"]}
tokio-util="0.7"
log="0.4"
''',encoding='utf-8')
(path/'src/lib.rs').write_text(r'''
pub type TauriResult<T> = anyhow::Result<T>;
pub mod tauri_plugin_opener { pub fn open_path<P:AsRef<std::path::Path>>(_:P,_:Option<&str>)->anyhow::Result<()> { Ok(()) } }
pub mod shared {
 use std::{path::PathBuf,sync::{LazyLock,atomic::{AtomicU64,Ordering}}};
 pub static WORKING_PATH:LazyLock<PathBuf>=LazyLock::new(||PathBuf::from(std::env::var("CONEST_TEST_STORAGE").unwrap()));
 pub enum Sidecar { FFmpeg }
 pub struct Handle; pub struct Paths;
 pub fn get_app_handle()->&'static Handle { &Handle }
 impl Handle { pub fn emit<T:serde::Serialize>(&self,_:&str,_:T)->anyhow::Result<()> {Ok(())} pub fn path(&self)->Paths{Paths} }
 impl Paths {pub fn resource_dir(&self)->anyhow::Result<PathBuf>{Ok(WORKING_PATH.clone())}}
 pub fn random_string(_:usize)->String{static COUNTER:AtomicU64=AtomicU64::new(0);format!("test{}",COUNTER.fetch_add(1,Ordering::SeqCst))}
}
pub mod storage {pub mod config {
 pub struct Proxy {pub address:String,pub username:String,pub password:String}
 pub struct Settings {pub proxy:Proxy,pub speed_limit:serde_json::Number,pub auto_download:bool}
 impl Settings { pub fn sidecar(&self,_:crate::shared::Sidecar)->std::path::PathBuf{std::env::var("CONEST_TEST_FFMPEG").unwrap_or("ffmpeg".into()).into()} }
 pub fn read()->Settings{Settings{proxy:Proxy{address:String::new(),username:String::new(),password:String::new()},speed_limit:0.into(),auto_download:false}}
}}
pub mod services {pub mod queue {pub mod runtime {
 use std::sync::{Arc,LazyLock};use tokio::sync::{Semaphore,RwLock};
 pub struct Runtime {pub semaphore:RwLock<Arc<Semaphore>>}
 pub static RUNTIME:LazyLock<Runtime>=LazyLock::new(||Runtime{semaphore:RwLock::new(Arc::new(Semaphore::new(2)))});
}}}
pub mod provider;
#[cfg(test)] mod integration {
 use super::provider::*;
 #[tokio::test] async fn real_download_cpu_transcode_and_saved_history() {
  let url=std::env::var("CONEST_FIXTURE_URL").unwrap();
  let settings=EngineSettings{executable:std::env::var("CONEST_TEST_YTDLP").unwrap(),..Default::default()};
  let collection=ytdlp_analyze(url,false,settings.clone()).await.unwrap();
  assert_eq!(collection.entries.len(),1);assert!(collection.entries[0].available);
  let options=DownloadOptions{mode:"video".into(),height:0,container:"mkv".into(),video_codec:"h264".into(),audio_codec:"aac".into(),bitrate:128,accelerator:"cpu".into(),output_dir:std::env::var("CONEST_TEST_OUTPUT").unwrap(),subtitles:String::new(),auto_subtitles:false,embed_subtitles:false,thumbnail:false,metadata:false,sponsorblock:false,section_start:None,section_end:None};
  let tasks=ytdlp_submit(collection.entries.clone(),options.clone(),settings.clone()).await.unwrap();
  assert_eq!(tasks[0].state,"backlog");
  ytdlp_control(tasks[0].id.clone(),"start".into()).await.unwrap();
  let done=tokio::time::timeout(std::time::Duration::from_secs(60),async{
   loop {let current=ytdlp_tasks().await.unwrap().into_iter().find(|t|t.id==tasks[0].id).unwrap();if ["completed","failed"].contains(&current.state.as_str()){break current;}tokio::time::sleep(std::time::Duration::from_millis(100)).await;}
  }).await.unwrap();
  assert_eq!(done.state,"completed","{}\n{:?}",done.error,done.log);
  assert!(std::path::Path::new(&done.output).is_file());assert!(done.output.ends_with("converted.mkv"));
  let saved:serde_json::Value=serde_json::from_slice(&std::fs::read(crate::shared::WORKING_PATH.join("Providers/yt-dlp-tasks.json")).unwrap()).unwrap();
  assert_eq!(saved[&done.id]["state"],"completed");
  // The same semaphore used by native Bilibili must gate external tasks.
  let sem=crate::services::queue::runtime::RUNTIME.semaphore.read().await.clone();
  let blocker=sem.acquire_many_owned(2).await.unwrap();
  let queued=ytdlp_submit(collection.entries.clone(),options.clone(),settings.clone()).await.unwrap();
  ytdlp_control(queued[0].id.clone(),"start".into()).await.unwrap();
  tokio::time::sleep(std::time::Duration::from_millis(100)).await;
  assert_eq!(ytdlp_tasks().await.unwrap().into_iter().find(|t|t.id==queued[0].id).unwrap().state,"queued");
  ytdlp_control(queued[0].id.clone(),"stop".into()).await.unwrap();
  tokio::time::timeout(std::time::Duration::from_secs(5),async {
    loop {if ytdlp_tasks().await.unwrap().into_iter().find(|t|t.id==queued[0].id).unwrap().state=="stopped" { break; }tokio::time::sleep(std::time::Duration::from_millis(50)).await;}
  }).await.unwrap();
  drop(blocker);
  // Interrupted tasks may change codecs/device before retry, but running tasks may not.
  let changed=ytdlp_edit(queued[0].id.clone(),options.clone(),settings.clone()).await.unwrap();
  assert_eq!(changed.options.accelerator,"cpu");
  assert!(ytdlp_edit(done.id.clone(),options.clone(),settings.clone()).await.is_err());

  // H.264 source -> WebM requires an MKV intermediate, then actual VP9/Opus.
  let mut webm=options.clone();webm.container="webm".into();webm.video_codec="vp9".into();webm.audio_codec="opus".into();
  let converted=ytdlp_submit(collection.entries.clone(),webm,settings.clone()).await.unwrap();
  ytdlp_control(converted[0].id.clone(),"start".into()).await.unwrap();
  let converted=tokio::time::timeout(std::time::Duration::from_secs(60),async {
    loop {let t=ytdlp_tasks().await.unwrap().into_iter().find(|t|t.id==converted[0].id).unwrap();if ["completed","failed"].contains(&t.state.as_str()) {break t;}tokio::time::sleep(std::time::Duration::from_millis(100)).await;}
  }).await.unwrap();
  assert_eq!(converted.state,"completed","{}\n{:?}",converted.error,converted.log);assert!(converted.output.ends_with("converted.webm"));
  let mut audio=options.clone();audio.mode="audio".into();audio.audio_codec="mp3".into();
  let audio=ytdlp_submit(collection.entries,audio,settings).await.unwrap();
  ytdlp_control(audio[0].id.clone(),"start".into()).await.unwrap();
  let audio=tokio::time::timeout(std::time::Duration::from_secs(60),async {
    loop {let t=ytdlp_tasks().await.unwrap().into_iter().find(|t|t.id==audio[0].id).unwrap();if ["completed","failed"].contains(&t.state.as_str()) {break t;}tokio::time::sleep(std::time::Duration::from_millis(100)).await;}
  }).await.unwrap();
  assert_eq!(audio.state,"completed","{}\n{:?}",audio.error,audio.log);assert!(audio.output.ends_with(".mp3"));


 }
}
''',encoding='utf-8')
print('Headless harness:',path,flush=True)
# Serve a generated local source; no account or public website is needed.
assets=path/'media';assets.mkdir()
ffmpeg=os.environ.get('CONEST_TEST_FFMPEG','ffmpeg')
ffmpeg=shutil.which(ffmpeg) or ffmpeg
subprocess.run([ffmpeg,'-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc=size=160x90:rate=10','-f','lavfi','-i','sine=frequency=440:sample_rate=44100','-t','1','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac',str(assets/'source.mp4')],check=True)
class Handler(http.server.SimpleHTTPRequestHandler):
 def __init__(self,*a,**kw):super().__init__(*a,directory=str(assets),**kw)
 def log_message(self,*a):pass
server=socketserver.TCPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
yt=os.environ.get('CONEST_TEST_YTDLP')
if not yt:
 import shutil
 yt=shutil.which('yt-dlp')
if not yt:
 if os.name=='nt' or importlib.util.find_spec('yt_dlp') is None:
  raise SystemExit('Install yt-dlp or set CONEST_TEST_YTDLP to its executable path.')
 wrapper=path/'yt-dlp'
 wrapper.write_text('#!'+sys.executable+'\nfrom yt_dlp import main\nmain()\n',encoding='utf-8');wrapper.chmod(0o755);yt=str(wrapper)
env=os.environ.copy();env.update(CONEST_FIXTURE_URL=f'http://127.0.0.1:{server.server_address[1]}/source.mp4',CONEST_TEST_STORAGE=str(path/'storage'),CONEST_TEST_OUTPUT=str(path/'output'),CONEST_TEST_YTDLP=yt,CONEST_TEST_FFMPEG=ffmpeg,NO_PROXY='127.0.0.1,localhost')
env['CARGO_TARGET_DIR']=str(root/'target/provider-headless')
try:result=subprocess.run([os.environ.get('CONEST_TEST_CARGO','cargo'),'test','--manifest-path',str(path/'Cargo.toml'),'--','--nocapture'],env=env)
finally:server.shutdown()
raise SystemExit(result.returncode)
