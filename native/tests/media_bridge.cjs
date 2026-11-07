const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

function player(initialVolume = .5, identity = {}, autoplay = true) {
  let now = 10000, interval, id = 'Fixture0001', clock = 10;
  let title='NOVA LANE - Night shift (Official Video)',songButton=null;
  const messages = [];
  const events = new Map(), playedVolumes = [];
  const emit = (event, target) => {for (const listener of events.get(event) || []) listener({target});};
  class MockMedia {
    constructor() {
      Object.assign(this,{readyState:4,seeking:false,paused:false,ended:false,duration:200,currentTime:10,
        _volume:1,muted:false,videoWidth:640,videoHeight:360});
    }
    get volume() {return this._volume;}
    set volume(value) {if(this._volume !== value) {this._volume=value; emit('volumechange',this);}}
    addEventListener() {}
    pause() {this.paused=true;}
    play() {playedVolumes.push(this.volume);emit('play',this);this.paused=false;return Promise.resolve();}
  }
  let media = new MockMedia(), apiVolume = 100;
  let acceptSeek = true,unstarted=false,acceptNavigation=true,navigations=0,playability='OK',advertisement=false;
  const api = {
    getVideoElement: () => media, getVideoData: () => ({video_id: id,title}),
    getCurrentTime: () => clock, getDuration: () => 200,
    getPlayerResponse:()=>({playabilityStatus:{status:playability}}),
    getPlayerState: () => unstarted ? -1 : media.paused ? 2 : 1,
    getVolume: () => apiVolume, setVolume(value) {apiVolume=value;media.volume=value/100;},
    isMuted: () => media.muted, mute() {media.muted=true;}, unMute() {media.muted=false;},
    seekTo(target) {
      if (acceptSeek) {
        clock = media.currentTime = target;
        media.seeking = true; media.readyState = 1;
      }
    },
    playVideo() {media.paused = false;}, pauseVideo() {media.paused = true;}
  };
  const document = {
    readyState: 'complete', head: {},
    addEventListener(event,listener) {if (!events.has(event)) events.set(event,[]);events.get(event).push(listener);},
    querySelectorAll: () => [media],
    querySelector(selector) {
      if (selector === 'ytmusic-player') return {playerApi: api};
      if (selector === 'ytmusic-player-bar') return {};
      if (selector === 'ytmusic-av-toggle .song-button') return songButton;
      return null;
    },
    getElementById: () => ({classList: {contains: () => advertisement}}),
    dispatchEvent(event) {navigations++;if(!acceptNavigation)return;id = event.detail.endpoint.watchEndpoint.videoId;if(id==='Fixture0001')title='NOVA LANE - Night shift (Official Video)';}
  };
  const context = vm.createContext({
    document, location: {origin: 'https://music.youtube.com'},
    window: {ytcfg:{get:key=>identity[key]},__rustifiedInitialVolume:initialVolume,ipc: {postMessage: text => messages.push(JSON.parse(text))}},
    HTMLMediaElement:MockMedia,
    performance: {now: () => now},
    CustomEvent: class {constructor(type, options) {Object.assign(this, options);}},
    setInterval(callback) {interval = callback;}
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/media_bridge.js'), 'utf8'), context);
  const command = value => context.window.__rustifiedMedia.command(value);
  const tick = (elapsed = 200) => {
    now += elapsed; interval();
    return messages.filter(message => message.type === 'state').at(-1);
  };
  if(autoplay) command({op: 'play', id, generation: 1}); tick();
  return {
    command, tick, messages, identity, get media() {return media;}, playedVolumes,
    replaceMedia() {media=new MockMedia();return media;},
    resetYouTubeVolume(value) {apiVolume=value*100;media.volume=value;},
    unstarted() {unstarted=true;},
    rejectNavigation(){acceptNavigation=false;},
    status(value){playability=value;},
    advert(value){advertisement=value;},
    get navigations(){return navigations;},
    enableAudioSwitch(nextTitle) {songButton={disabled:false,getAttribute:()=>null,hasAttribute:()=>false,click(){id='Fixture0002';title=nextTitle;clock=media.currentTime=0;}};},
    get id(){return id;},
    rejectSeek() {acceptSeek = false;},
    clock(value) {clock = media.currentTime = value;},
    handoff() {id = 'Fixture0002'; clock = media.currentTime = 0; media.seeking = false; media.readyState = 4;}
  };
}

test('a seek is acknowledged only after the selected clock settles', () => {
  const p = player();
  p.command({op: 'seek', value: 90, revision: 1});
  assert.equal(p.tick().seek_ack, 0);
  p.media.seeking = false; p.media.readyState = 4;
  assert.equal(p.tick().seek_ack, 1);
});

test('YouTube volume resets are capped synchronously without waiting for a poll', () => {
  const p=player(.2);
  p.resetYouTubeVolume(1);
  assert.equal(p.media.volume,.2);
  p.media.play();
  assert.ok(p.playedVolumes.every(volume => volume<=.2));
});

test('new media elements are limited before their first play call', () => {
  const p=player(.25), replacement=p.replaceMedia();
  assert.equal(replacement.volume,1);
  replacement.play();
  assert.deepEqual(p.playedVolumes,[.25]);
});

test('normalization below the selected level is preserved across polls', () => {
  const p=player(.6);
  p.media.volume=.35;
  p.tick();
  assert.equal(p.media.volume,.35);
});

test('mute survives YouTube unmute and volume restores, then user volume can increase', () => {
  const p=player(0);
  p.resetYouTubeVolume(1);p.media.muted=false;p.media.play();
  assert.equal(p.media.volume,0);
  assert.deepEqual(p.playedVolumes,[0]);
  p.command({op:'volume',value:.7});
  assert.equal(p.media.volume,.7);assert.equal(p.media.muted,false);
});

test('rapid seeks confirm the latest request, never the old clock', () => {
  const p = player();
  p.command({op: 'seek', value: 40, revision: 1});
  p.command({op: 'seek', value: 140, revision: 2});
  p.media.seeking = false; p.media.readyState = 4; p.clock(40);
  assert.equal(p.tick().seek_ack, 0);
  p.clock(140);
  assert.equal(p.tick().seek_ack, 2);
});

test('an unaccepted seek near the end cannot skip the current track', () => {
  const p = player(); p.rejectSeek();
  p.command({op: 'seek', value: 199, revision: 1});
  const state = p.tick(1000);
  assert.equal(state.ended, false);
  assert.equal(state.position, 10);
  assert.equal(state.seek_ack, 0);
});

test('a final-frame seek hands control back to the native queue before seeked', () => {
  const p = player();
  p.command({op: 'seek', value: 199, revision: 1});
  assert.equal(p.tick().ended, false);
  p.handoff();
  const state = p.tick(600);
  assert.equal(state.ended, true);
  assert.equal(state.playing, false);
  assert.equal(p.media.paused, true);
});


test('playback identity is reported without a song and unknown is distinct from signed out', () => {
  const p=player(.5,{},false);
  let session=p.messages.filter(m=>m.type==='session').at(-1);
  assert.equal(session.signed_in,null);assert.equal(session.auth_user,null);
  p.identity.LOGGED_IN=false;p.tick();
  session=p.messages.filter(m=>m.type==='session').at(-1);
  assert.equal(session.signed_in,false);
  p.identity.LOGGED_IN=true;p.identity.SESSION_INDEX='2';p.tick();
  session=p.messages.filter(m=>m.type==='session').at(-1);
  assert.equal(session.signed_in,true);assert.equal(session.auth_user,2);
  assert.ok(session.document_id);
});

test('invalid and missing account selectors are never coerced to account zero', () => {
  for(const index of [undefined,null,'invalid',-1,256]) {
    const p=player(.5,{LOGGED_IN:true,SESSION_INDEX:index});
    assert.equal(p.messages.find(m=>m.type==='session').auth_user,null);
  }
});

test('reconnecting restores a paused song only after its seek settles', () => {
  const p=player(.2);
  p.command({op:'play',id:'Fixture0001',generation:1,position:90,revision:7,paused:true});
  let state=p.tick();assert.equal(state.transition,true);assert.equal(state.seek_ack,0);
  assert.equal(p.media.paused,true);
  p.media.seeking=false;p.media.readyState=4;
  state=p.tick();assert.equal(state.seek_ack,7);assert.equal(state.position,90);
  assert.equal(state.transition,false);assert.equal(state.playing,false);
  assert.equal(p.media.volume,.2);
});


test('a stale audio toggle cannot replace the selected song with another radio item', () => {
  const p=player(.2);p.clock(32);
  p.enableAudioSwitch('NOVA LANE - Glass harbor (Official Audio)');
  p.command({op:'mode',video:false});p.tick();
  assert.equal(p.id,'Fixture0001');
  p.tick();p.media.seeking=false;p.media.readyState=4;
  const state=p.tick(2000);
  assert.equal(p.id,'Fixture0001');assert.equal(state.position,32);
  assert.ok(state.title.includes('Night'));assert.equal(p.media.volume,.2);
});

test('the real audio counterpart is accepted when song titles match', () => {
  const p=player();p.enableAudioSwitch('NOVA LANE - Night shift (Official Audio)');
  p.command({op:'mode',video:false});p.tick();
  assert.equal(p.id,'Fixture0002');
  assert.equal(p.tick(2000).transition,false);
});


test('a paused unstarted player can finish an audio mode transition before restoring its seek', () => {
  const p=player();p.media.pause();p.unstarted();
  p.command({op:'pause'});
  p.enableAudioSwitch('NOVA LANE - Night shift (Official Audio)');
  p.command({op:'mode',video:false});
  p.command({op:'seek',value:45,revision:9});
  assert.equal(p.tick().transition,true);
  p.tick(2000);p.media.seeking=false;p.media.readyState=4;
  const state=p.tick();assert.equal(state.transition,false);assert.equal(state.seek_ack,9);
  assert.equal(state.position,45);assert.equal(state.playing,false);
});


test('ignored SPA navigation requests one recovery without accusing the account of being signed out',()=>{
  const p=player();p.rejectNavigation();
  p.command({op:'play',id:'Fixture0002',generation:2});p.tick(5000);p.tick(5000);
  const stalls=p.messages.filter(m=>m.type==='stalled');
  assert.equal(stalls.length,1);assert.equal(stalls[0].video_id,'Fixture0002');
  assert.equal(stalls[0].generation,2);assert.equal(stalls[0].status,'NAVIGATION_STALLED');
  assert.ok(stalls[0].document_id);
  assert.equal(p.media.paused,true);
});

test('a direct watch reload does not dispatch the failed SPA navigation again',()=>{
  const p=player();const count=p.navigations;
  p.command({op:'play',id:'Fixture0001',generation:2,direct:true,paused:true});
  p.tick();assert.equal(p.navigations,count);
});

test('only an explicit server login requirement requests account reconnection',()=>{
  const p=player();p.status('LOGIN_REQUIRED');p.tick(1200);
  assert.equal(p.messages.filter(m=>m.type==='stalled').at(-1).status,'LOGIN_REQUIRED');
});

test('an advertisement is never treated as stalled song navigation',()=>{
  const p=player();p.advert(true);p.rejectNavigation();
  p.command({op:'play',id:'Fixture0002',generation:2});p.tick(20000);
  assert.equal(p.messages.filter(m=>m.type==='stalled').length,0);
});


test('an old track login response cannot mark a new requested track as signed out',()=>{
  const p=player();p.rejectNavigation();p.status('LOGIN_REQUIRED');
  p.command({op:'play',id:'Fixture0002',generation:2});p.tick(5000);
  assert.equal(p.messages.filter(m=>m.type==='stalled').at(-1).status,'NAVIGATION_STALLED');
});
