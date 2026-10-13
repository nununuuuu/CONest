import assert from 'node:assert/strict';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import path from 'node:path';
import {createRequire} from 'node:module';
import {createServer} from 'vite';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';

// Deterministic OS/provider adapters; production Vue templates, virtual list and CSS.
const root=process.cwd(), fixture=await mkdtemp(path.join(root,'ui-qa-'));
const mock=path.join(fixture,'mock.js');
const require=createRequire(path.join(process.env.CONEST_UI_QA_MODULE,'package.json'));
const {chromium}=require('playwright');
await writeFile(mock, [
"import {reactive} from 'vue';",
"export const fixtureState=reactive({loading:false});",
"const settings=reactive({down_dir:'C:/Downloads/很長的路徑/'.repeat(4),add_metadata:true,max_conc:3,isDark:true,theme:'dark'});",
"const tasks={tasks:{}};",
"export const useSettingsStore=()=>settings;",
"export const useUserStore=()=>({isLogin:false,name:''});",
"export const useQueueStore=()=>tasks;",
"export const useComponentsStore=()=>({c:{},navigate:async()=>({})});",
"const collection={title:'YouTube 播放清單：长標題不能擠壓設定 '.repeat(6),platform:'YouTube',is_playlist:true,entries:Array.from({length:90},(_,i)=>({index:i+1,id:'item-'+i,title:'影片 '+(i+1)+' · 長標題 '.repeat(10),available:i!==2,duration:180,thumbnail:null})),warnings:'部分項目無法取得，原始位置保留。'};",
"export const useYtdlpStore=()=>({...tasks,analyze:async()=>collection,submit:async()=>{}});",
"export const message=e=>String(e);",
"const mediaInfo={type:'video',id:'BV1xx411c7mD',pn:true,nfo:{showtitle:'長標題：內容清單與設定欄需要各自獨立 '.repeat(6),intro:'完整介紹可獨立捲動，不能覆蓋內容清單。\\n'.repeat(15),tags:[],url:'',stat:{play:100000,danmaku:300,reply:200,like:5000,coin:300,favorite:1000,share:80},thumbs:[],upper:{name:'作者名稱',mid:1,avatar:''}},sections:{target:1,tabs:[{id:1,name:'全部內容'},{id:2,name:'另一個長名稱的分類'}]},list:Array.from({length:160},(_,index)=>({index,title:'第 '+(index+1)+' 項 · 很長的影片標題 '.repeat(6),aid:index+1,cid:index+1,isTarget:index===0}))};",
"export const data={getMediaInfo:async()=>{await new Promise(r=>setTimeout(r,fixtureState.loading?1800:50));return structuredClone(mediaInfo);},getCid:async()=>1};",
"export const extras={};",
"export const commands={exportData:async()=>({status:'ok'})};",
"export const parseId=async()=>({id:'BV1xx411c7mD',type:'video'});",
"export const strip=s=>s;",
"export const stat=n=>String(n);",
"export const AppLog=()=>{};",
"export const getBlob=async s=>s;",
"export const openUrl=async()=>{};",
"export const open=async()=>null;",
"export const save=async()=>null;",
"export const info=()=>{};",
"export const submit=()=>{};",
"export const submitExtensionDownload=async()=>{};",
"export const route=reactive({path:'/'});",
"export default {push:async p=>{route.path=p;},global:{t:k=>k}};"
].join('\n'));
await writeFile(path.join(fixture,'components.js'),"export {default as Empty} from '/src/components/Empty.vue'; export {default as Dropdown} from '/src/components/Dropdown.vue';");
await writeFile(path.join(fixture,'search-components.js'),"export {default as MediaInfo} from '/src/components/SearchPage/MediaInfo.vue'; export {default as MediaList} from '/src/components/SearchPage/MediaList.vue';");
await writeFile(path.join(fixture,'index.html'),'<html class="dark"><div id="app"></div><script type="module" src="./main.js"></script></html>');
await writeFile(path.join(fixture,'main.js'),[
"import {createApp,h,ref,nextTick} from 'vue';",
"import Header from '/src/components/WorkspaceHeader.vue';",
"import SearchPage from '/src/views/SearchPage.vue';",
"import '/src/style.css';",
"import {route,fixtureState} from './mock.js';",
"const page=ref();",
"const app=createApp({setup:()=>()=>h('div',{id:'main'},[h('div',{class:'workspace-titlebar'}),h(Header),h(SearchPage,{ref:page,class:'page'})])});",
"app.config.globalProperties.$t=k=>({'search.suggest':'貼上連結，開始選取內容','search.general':'完整下載設定','search.export':'匯出解析資料','search.selectAll':'全選／取消全選','search.autoDetect':'自動偵測','empty':'沒有可下載的項目'}[k]||k);",
"app.config.globalProperties.$fa={weight:'fa-light'};",
"app.config.globalProperties.$route=route;",
"app.mount('#app');",
"window.fixture={search:async(input,loading=false)=>{fixtureState.loading=loading;return page.value.search(input);},theme:async t=>{document.documentElement.className=t;await nextTick();}};"
].join('\n'));
const names=['@/store','@/services/ytdlp','@/services/backend','@/services/utils','@/services/media','@/services/queue','@/services/extensionDownload','@/router','@/i18n','@tauri-apps/plugin-dialog','@tauri-apps/plugin-log','@tauri-apps/plugin-opener'];
const server=await createServer({configFile:false,root,logLevel:'warn',plugins:[vue(),tailwindcss()],
resolve:{alias:[
...names.map(find=>({find:new RegExp('^'+find.replace(/[.*+?^{}()|[\]\\$]/g,'\\$&')+'$'),replacement:mock})),
{find:/^@\/components$/,replacement:path.join(fixture,'components.js')},
{find:/^@\/components\/SearchPage$/,replacement:path.join(fixture,'search-components.js')},
{find:'@',replacement:path.join(root,'src')}]},
server:{host:'127.0.0.1',port:1430,strictPort:true}});
let browser;
const errors=[];
try {
 await server.listen();
 browser=await chromium.launch({channel:'msedge',headless:true});
 const page=await browser.newPage();
 page.on('pageerror',e=>errors.push(String(e)));
 const url='http://127.0.0.1:1430/'+path.basename(fixture)+'/index.html';
 for(const theme of ['dark','light']) for(const size of [{width:1280,height:800},{width:1000,height:680},{width:820,height:600},{width:640,height:760}]) {
  await page.setViewportSize(size);
  await page.goto(url);
  await page.waitForFunction(()=>Boolean(window.fixture));
  await page.evaluate(t=>window.fixture.theme(t),theme);
  for(const scenario of ['empty','native','playlist','loading']) {
   if(scenario==='native') await page.evaluate(()=>window.fixture.search('BV1xx411c7mD'));
   if(scenario==='playlist') {
    await page.evaluate(()=>window.fixture.search('https://www.youtube.com/playlist?list=test'));
    await page.waitForSelector('.workspace-media-row');
    await page.locator('.workspace-settings-scroll details').evaluateAll(ds=>ds.forEach(d=>d.open=true));
   }
   if(scenario==='loading') {
    await page.evaluate(()=>{void window.fixture.search('BV1xx411c7mD',true);});
    await page.waitForSelector('.workspace-spinner');
   }
   await page.evaluate(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))));
   const m=await page.evaluate(()=>{
    const rect=s=>{const r=document.querySelector(s)?.getBoundingClientRect();return r&&{x:r.x,y:r.y,w:r.width,h:r.height,right:r.right,bottom:r.bottom};};
    const scroll=document.querySelector('.workspace-settings-scroll');
    return {list:rect('.workspace-content'),settings:rect('.workspace-inspector'),dock:rect('.workspace-dock'),input:rect('.workspace-import'),
     overflow:document.querySelector('#main').scrollWidth>innerWidth+1,
     images:document.querySelectorAll('img[src*="empty.png"],img[src*="searching.png"]').length,
     native:rect('.workspace-native-list'),footer:rect('.workspace-settings-footer'),scrollHeight:scroll?.scrollHeight,clientHeight:scroll?.clientHeight};
   });
   assert.equal(m.images,0);
   assert.equal(m.overflow,false,JSON.stringify({theme,size,scenario,m}));
   assert.ok(m.list&&m.settings);
   if(size.width>760) {
    assert.ok(m.list.right<=m.settings.x+1,'Content and settings overlap');
    assert.ok(m.list.y>=m.input.bottom-1,'Content overlaps import');
    assert.ok(m.list.bottom<=m.dock.y+1,'Content overlaps task dock');
    assert.ok(m.dock.bottom<=size.height+1,'Task dock outside viewport');
    if(scenario==='playlist') {
     assert.ok(m.scrollHeight>m.clientHeight,'Settings must scroll');
     assert.ok(m.footer.bottom<=m.dock.y+1,'Queue button hidden');
    }
    if(scenario==='native') assert.ok(m.native.h>=100,'Native list lost scroll area');
   } else assert.ok(m.settings.y>=m.list.bottom-1,'Stacked panels overlap');
   if(scenario==='playlist') assert.equal(await page.locator('.workspace-settings-footer button').isEnabled(),true);
   console.log('PASS',theme,size.width+'x'+size.height,scenario);
   if(scenario==='loading') await page.waitForSelector('.workspace-spinner',{state:'detached'});
  }
 }
 assert.deepEqual(errors,[],'Browser runtime errors');
 console.log('Workspace layout: 32 real-component browser cases passed');
} finally {
 await browser?.close();
 await server.close();
 await rm(fixture,{recursive:true,force:true});
}
